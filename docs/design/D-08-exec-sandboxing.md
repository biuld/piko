# D-08: PTY process lifecycle, shell snapshots, and network sandbox

> Status: accepted
> Implements: [F-08](../features/F-08-exec-sandboxing.md) (slice 1)

## Goal

Replace the unsandboxed `bash` tool path (plain `tokio::process::Command`
behind a static command-name check) with a PTY-backed, process-group-aware
runner in `piko-sandbox`, wired through the existing `WorkspaceToolProvider`.
The slice delivers:

1. Async PTY spawn with the shell as session/process-group leader, bounded
   combined output, timeout, and cancellation grace (SIGTERM → SIGKILL to the
   whole group).
2. Shell snapshot resolution (`shell_path` → `$SHELL` → platform default)
   with cwd/env captured at bootstrap and reused per call.
3. Network allow/deny from `Policy.allow_network` on both macOS (seatbelt)
   and Linux (bwrap), including `--share-net` when allowed.

## Constraints and non-goals

- `piko-sandbox` stays the fail-closed boundary. `piko-orchd` only translates
  tool arguments into `SpawnConfig`; it never spawns raw processes itself.
- hostd stays authoritative for settings; the sandbox config (`enabled`,
  `policy_path`, `shell_path`) continues to flow through
  `SandboxConfig`/`SandboxSettings` unchanged. Slice 1 adds no new config
  keys (kill grace and output cap are runner defaults, overridable in code).
- The OS sandbox wrapper is used only when the sandbox is *enabled*
  (`SandboxConfig.enabled == true`); a disabled sandbox means direct
  execution with PTY lifecycle but no seatbelt/bwrap wrapper (operator
  choice, permissive policy already signals this today).
- Non-goals from the PRD: unified long-lived processes, environment
  capability selection, network proxy, Windows backends, cwd tracking.
- No new crate dependencies beyond `tokio`/`tokio-util`/`libc`, all already
  in the workspace lockfile — the design must build offline.

## Proposed design

### `piko-sandbox`: new `exec` module

New file `packages/sandbox/src/exec.rs`, exported from `lib.rs`.

```rust
#[derive(Debug, Clone)]
pub struct ShellSnapshot {
    pub shell_path: String,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

impl ShellSnapshot {
    /// Resolve the shell: configured override → $SHELL → platform default.
    pub fn resolve(configured: Option<&str>) -> String;
    /// Capture cwd + env at bootstrap. Always captures HOME and PATH; drops
    /// DYLD_*/LD_* loader variables (stripped by seatbelt anyway).
    pub fn capture(shell_path: String) -> Self;
}

#[derive(Debug, Clone)]
pub struct SpawnConfig {
    pub command: String,
    pub shell: ShellSnapshot,
    /// `Some(policy)` wraps execution in the platform OS sandbox
    /// (seatbelt on macOS, bwrap on Linux); `None` runs directly.
    pub policy: Option<Policy>,
    pub timeout: Option<Duration>,
    pub cancel: Option<tokio_util::sync::CancellationToken>,
    pub kill_grace: Duration,        // default 2s
    pub max_output_bytes: usize,     // default 65_536
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus { pub code: Option<i32>, pub signal: Option<i32> }

#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutcome {
    pub status: ExitStatus,
    pub output: String,
    pub truncated: bool,
    pub timed_out: bool,
    pub cancelled: bool,
}

pub async fn run(config: SpawnConfig) -> Result<CommandOutcome, ExecError>;
```

`ExecError` covers PTY allocation and spawn failures; termination outcomes
(exit/signal/timeout/cancel) are data, not errors, so the tool layer can
commit a bounded result in every case.

### PTY lifecycle (Unix)

1. `posix_openpt(O_RDWR | O_NOCTTY)` → master; `grantpt`/`unlockpt`/
   `ptsname` → slave; open the slave *without* `O_NOCTTY` so the child can
   acquire it as the controlling terminal. The slave fd is duplicated three
   times for stdin/stdout/stderr; the original is kept for `TIOCSCTTY`.
2. The command line is either the bare `<shell> -c <command>` or, when
   `policy` is present, the platform wrapper around it:
   - macOS: `/usr/bin/sandbox-exec -p <policy> [-Dkey=value...] -- <shell> -c <command>`,
     skipping the wrapper when `APP_SANDBOX_CONTAINER_ID` is set (nested
     sandbox detection from the existing runner).
   - Linux: `bwrap --unshare-all ... [--share-net] -- <shell> -c <command>`.
3. The spawned wrapper/shell is configured with `.process_group(0)` and a
   `pre_exec` that calls `setsid()` + `ioctl(fd, TIOCSCTTY, 0)` on the slave
   fd, making the wrapper the session leader and the whole subtree one
   process group. `cwd` comes from the snapshot; `env` overrides are applied.
4. After `spawn()`, all slave fds are closed in the parent (otherwise EOF on
   the master never arrives). The master fd is set `O_NONBLOCK` and wrapped
   in a `tokio::fs::File`.
5. A reader task drains the master into a `Vec<u8>` capped at
   `max_output_bytes`; past the cap it keeps draining (so a chatty child
   cannot deadlock on a full PTY buffer) and sets `truncated`.

### Termination

`tokio::select!` races the child wait, the timeout sleep, and the
cancellation token:

- child exits first → drain the reader with a short grace (background
  grandchildren may keep the PTY open), map `ExitStatusExt` to
  `{ code, signal }`, and return.
- timeout or cancel fires → mark the outcome, `libc::kill(-pid, SIGTERM)`,
  race the wait against `kill_grace`; if the grace elapses,
  `libc::kill(-pid, SIGKILL)` and await the wait. The negative pid targets
  the process group, so children die with the shell.

The result distinguishes `timed_out` and `cancelled` so hostd/orchd can
commit the right bounded error code in the transcript (F-06 contract).

### Network policy

- macOS: `runner.rs` already emits `(allow network-outbound)` /
  `(allow network-inbound)` only when `allow_network` is true against the
  deny-by-default seatbelt base. The new `exec` path reuses the same policy
  builder (extracted into a shared helper) so the wrapper and the PTY runner
  cannot drift.
- Linux: bwrap stays on `--unshare-all` (network namespace unshared) when
  `allow_network` is false; when true, append `--share-net` instead of
  erroring out.

### `piko-orchd` wiring

- `load_sandbox_policy` becomes `Option<Policy>`: `None` when the sandbox is
  disabled (direct execution), `Some(policy)` otherwise. The bootstrap passes
  the option into `WorkspaceToolProvider`, which stores it alongside a
  resolved `ShellSnapshot` (resolved once at provider construction).
- The `bash` handler in `workspace_handlers.rs` stops using
  `tokio::process::Command`/`timeout` and calls
  `piko_sandbox::exec::run(SpawnConfig { timeout, cancel:
  context.cancellation.clone(), .. })`. `policy.validate_command` remains as
  a fast-fail pre-check (cheap, deterministic) but is no longer the security
  boundary.
- Result mapping: `ok` on exit code 0, value `{ output, exitCode, signal,
  timedOut, cancelled }`; bounded error `command_failed`/`timed_out`/
  `cancelled`/`policy_violation` otherwise, preserving truncation at 50 KiB
  in the tool layer.

## Verification plan

- `piko-sandbox` unit tests (always on, no wrapper needed):
  snapshot resolution precedence; cwd/env passthrough; `exit 42` → code 42;
  `kill -KILL $$` → signal 9; timeout with `sleep 30` terminates fast;
  pre-cancelled token → cancelled; a background child dies with the group
  (capture `$$` via output, assert `kill -0 -<pgid>` fails after timeout).
- Wrapper tests (gated on the platform backend being available):
  network probe with `allow_network` false fails and with true succeeds;
  filesystem denial under seatbelt/bwrap.
- `piko-orchd` registry tests keep passing (bash runs through the runner).
- Evidence recorded in [V-08](../verification/V-08-exec-sandboxing.md), with
  `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` green.

## Implementation notes

- **PTY reads use `AsyncFd`, not `tokio::fs::File`.** A non-blocking PTY
  master wrapped in `tokio::fs::File` breaks the read loop on the first
  EAGAIN, so small outputs are lost and chatty children deadlock on a full
  PTY buffer. The reader awaits `AsyncFd::readable()` and reads inside
  `try_io`; the buffer cap drains-and-discards past `max_output_bytes` so a
  child can never block on a full buffer.
- **Command assembly bug caught in tests.** The first implementation passed
  the shell path twice in the direct (no-wrapper) case (`bash /bin/bash -c
  ...`), which made bash treat its own binary as a script file and exit
  126/127. The wrapper and direct cases now assemble argv separately.
- **Seatbelt platform defaults keep temp dirs writable.** `platform_defaults.sbpl`
  allows writes to `/tmp`, `/private/tmp`, `/var/tmp` (needed for normal
  tooling); the filesystem-denial acceptance probe therefore targets a
  read-only root (`/usr`) rather than `/tmp`.
- **Seatbelt does not filter loopback.** A deny-default policy still permits
  connections to `127.0.0.1`; the network acceptance probe uses an external
  address (`1.1.1.1:9`) where denial surfaces as `EPERM` (errno 1) and
  allowance as a successful `connect_ex`.
- **Nested sandboxes skip wrapper tests.** `sandbox-exec` cannot apply a
  second seatbelt policy (`sandbox_apply: Operation not permitted`) inside
  this dev harness; the macOS wrapper tests probe with a trivial policy and
  skip, and the unsandboxed run supplies the evidence (see V-08).
- **No fallback on wrapper failure.** If the OS sandbox cannot be applied,
  the run fails closed (exit 71 / spawn error surfaced to the tool layer)
  instead of silently executing unsandboxed; only the documented
  `APP_SANDBOX_CONTAINER_ID` nested-sandbox detection and the SIGABRT retry
  bypass the wrapper.

## Decisions

| Question | Decision | Rationale |
|---|---|---|
| PTY in `piko-sandbox` or a new crate? | Extend `piko-sandbox` | One fail-closed boundary; no new crate surface |
| tokio in `piko-sandbox`? | Yes (`process`, `io-util`, `time`, `sync`, `macros`, `rt`, `fs`) + `tokio-util` + `libc` | All already in the lockfile; orchd already depends on tokio |
| Wrapper or direct when sandbox disabled | Direct | Matches today's permissive-policy semantics; operator choice |
| OS wrapper process vs shell as group leader | Wrapper is the leader (pre_exec setsid); group = wrapper + shell + children | `kill(-pid)` covers the whole subtree without leaking policy to grandchildren |
| Cancel/timeout result | Data in `CommandOutcome` (never a thrown error) | Transcript stays bounded and complete (F-06) |
