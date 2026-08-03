# D-19: Long-lived processes and environment selection

> Status: accepted
> Implements: [F-08](../features/F-08-exec-sandboxing.md) (slice 2)

## Goal

Complete F-08 by making process execution *stateful across tool calls* and
making the execution environment a discovered, model-visible capability:

1. A [`ProcessManager`] in `piko-sandbox` owns PTY processes started by the
   workspace `process` tool — `start` (optional cwd/env overrides),
   `write_stdin`, incremental output reads, group `stop`, and `list`.
2. Environment capability discovery resolves a *usable* shell (configured →
   `$SHELL` → candidates), normalizes `PATH`, and probes common tools,
   surfaced to the model through a read-only `environment` tool.

## Constraints and non-goals

- `piko-sandbox` stays the process owner: the provider holds an
  `Arc<ProcessManager>` but never spawns raw processes itself.
- hostd stays authoritative for durable state; long-lived processes are
  transient orchd runtime state (they die with the orchestrator's provider).
- No new protocol/configuration keys; `SandboxConfig` is unchanged.
- Non-goals: network proxy / per-command network decisions, Windows
  backends, per-command PATH canonicalization, background
  detach-from-provider processes (processes always belong to a provider).
- No new crates: `tokio` (already a dependency) powers the async I/O; the
  existing `AsyncFd`-based PTY reader is reused.

## Proposed design

### `piko-sandbox`: `exec/process.rs`

`ProcessManager` (one per provider) keys processes by `proc-N` ids:

```rust
pub struct ProcessManager {
    processes: Mutex<HashMap<String, Arc<PtyProcess>>>,
    next_id: AtomicU64,
}
```

- `start(SpawnConfig)` reuses the slice-1 spawn path: extract
  `exec/unix::spawn_pty` (PTY + session-leader pre_exec + OS-sandbox
  wrapper) so the one-shot runner and the long-lived path share one command
  line. The spawned child is not awaited to completion; a **reaper task**
  waits on `child.wait()` and records the `ExitStatus`, and a **reader task**
  drains the master into a bounded unread buffer (cap
  `DEFAULT_MAX_OUTPUT_BYTES` = 256 KiB, drain-and-discard past it).
- `PtyProcess` exposes `write_stdin` (via `AsyncFd::writable`), non-blocking
  `try_read_output` (drains unread + reports `truncated`/`exited`/status),
  `wait_for_exit(timeout)`, and `stop(grace)` — SIGTERM to the group, then
  SIGKILL after the grace period (same escalation as slice 1).
- `Drop for ProcessManager` synchronously SIGKILLs every live group
  (plain syscall; safe after the runtime is gone) so provider teardown does
  not leak processes.

### `piko-sandbox`: `exec/env.rs`

```rust
pub struct EnvironmentProfile {
    pub shell: String,          // usable, validated
    pub cwd: PathBuf,
    pub path: Vec<PathBuf>,     // deduplicated, order-preserving
    pub os: &'static str,
    pub arch: &'static str,
    pub tools: Vec<String>,     // base names found via `command -v`
}
```

- `resolve_shell` walks configured → `$SHELL` → `[bash, zsh, sh, fish]` and
  picks the first that is executable (`access(X_OK)` for absolute paths,
  manual PATH search otherwise).
- `ShellSnapshot::capture` now derives its shell, cwd, and normalized PATH
  from the profile and guarantees `TERM` for PTY tools.
- Tools are probed once with a single `sh -c "command -v a; command -v b; …"`
  run; results are base names only (never paths with secrets).

### `piko-orchd`: provider ownership

`WorkspaceToolProvider` gains `processes: Arc<ProcessManager>` and
`env: EnvironmentProfile`, both resolved once at construction. The provider
routes by tool name: `bash` → `shell_handlers`, `process` →
`process_handlers`, `environment` → profile JSON, everything else →
`workspace_handlers` (file tools). `workspace_handlers.rs` was split to keep
each file under the size ceiling:

- `workspace_handlers.rs` — catalog (read/bash/edit/write/process/environment
  defs) + file tool handlers.
- `shell_handlers.rs` — the `bash` tool (moved from workspace_handlers).
- `process_handlers.rs` — the `process` tool (start/write/read/stop/list).

`process start` builds a `SpawnConfig` from the shell snapshot plus optional
cwd/env argument overrides, validates the command against the policy, and
applies the OS-sandbox wrapper when enabled — identical semantics to `bash`,
without a timeout (the process is expected to outlive the turn; `stop`
terminates it).

## Verification plan

- `piko-sandbox` process tests: incremental output accumulation, stdin round
  trip (`cat`), group termination (`echo $$; sleep 30 & wait` → `kill -0`
  probe fails), and list/get bookkeeping.
- `piko-sandbox` env tests: PATH dedup order, shell fallback chain, profile
  constants/tools structure.
- `piko-orchd` process-tool tests: start → write → read → stop → list round
  trip, unknown-process error, policy-violation block.
- Evidence in [V-19](../verification/V-19-exec-sandboxing-long-lived-processes.md),
  with `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace` green.

## Decisions

| Question | Decision | Rationale |
|---|---|---|
| Who owns long-lived processes? | `piko-sandbox::ProcessManager`, held by the workspace provider | Keeps the fail-closed boundary and process lifecycle in one crate; provider is the natural per-session owner |
| Process cleanup | `Drop` SIGKILLs all groups synchronously | No leaks when the provider/runtime goes away; kill is a plain syscall |
| Tool surface | One `process` tool with start/write/read/stop/list actions | Unified-exec style; one approval/capability entry point |
| Output model | Unread buffer drained per `read`; bounded at 256 KiB | Model sees deltas, never replays; cap prevents unbounded memory |
| Env selection scope | Validated shell + PATH normalization + tool probing | Covers codex `environment_selection`/`exec_env` distilled behavior without leaking process env/secrets |
