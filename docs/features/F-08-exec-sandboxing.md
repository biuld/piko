# F-08: Command execution & sandboxing

> Status: implemented (slice 1: PTY/process-group lifecycle, shell snapshots,
> network sandbox; slice 2: unified long-lived processes, environment
> capability selection)
> Priority: P1
> Source evidence: codex-rs `core/src/exec.rs`, `core/src/spawn.rs`,
> `core/src/shell.rs`, `core/src/shell_snapshot.rs`,
> `core/src/command_canonicalization.rs`, `core/src/exec_env.rs`,
> `core/src/environment_selection.rs`, `core/src/sandboxing/`,
> `core/src/sandbox_tags.rs`, `core/src/unified_exec/*`, `core/src/network_proxy`

## Summary

Real work happens when the agent can run commands. F-08 makes command
execution a first-class, fail-closed capability: every command runs inside a
PTY session as a process group, is bounded by a timeout with cancellation
grace, reuses a stable shell snapshot, and is confined by the filesystem and
network policy of `piko-sandbox`. The entry slice replaces the current
unsandboxed `bash` tool path (plain `tokio::process::Command` behind a static
command-name check) with the sandbox runner and adds process-group lifecycle,
shell-snapshot resolution, and explicit network allow/deny on both macOS and
Linux. Slice 2 completes the block: a `ProcessManager` keeps PTY processes
alive across tool calls (`write_stdin`, incremental output reads, group
termination) exposed through a `process` tool, and environment capability
discovery (usable shell resolution, PATH normalization, common-tool probing)
is surfaced through an `environment` tool.

## Problem

1. **The `bash` tool is not actually sandboxed.** `piko-orchd` runs
   `tokio::process::Command` directly with only `policy.validate_command` — a
   static allowlist of command names — in front. A shell script can touch
   anything the daemon user can, and network access is always available. The
   `piko-sandbox` runner (seatbelt/bwrap) exists but is not wired into the
   tool path at all.
2. **Commands have no process-group lifecycle.** Killing a long command
   requires an external `timeout` binary, which does not reliably kill the
   child processes a shell spawns (no process group, no signal cascade), and
   runtime cancellation (turn abort) never reaches the running process.
3. **Output is captured after the fact.** The current path uses
   `Command::output()`, so there is no PTY, no streaming, and no way to
   distinguish a signal death from an exit code. Interactive programs and
   tools that need a TTY fail or misbehave.
4. **The shell is an untracked constant.** `shell_path` defaults to `"bash"`
   with no discovery from the environment and no snapshot of cwd/env reused
   across calls, so commands can drift from the environment the user
   actually configured.
5. **Network policy is incomplete.** macOS seatbelt gates outbound network
   on `allow_network` only when a policy file is used; Linux bwrap always
   denies network and errors out when `allow_network` is requested instead of
   permitting it.

## User journeys

1. An agent runs `bash` to build a project. The command executes inside a
   PTY as a session-leading process group under the sandbox policy; output is
   streamed, bounded, and returned with exit code and signal info.
2. A build hangs. The agent's turn is cancelled; the runtime signals the
   whole process group (SIGTERM), waits the grace period, then escalates to
   SIGKILL, and the transcript records a bounded cancelled result.
3. An operator configures `[sandbox] shell_path = "/bin/zsh"`. Every `bash`
   tool call resolves the snapshot once at bootstrap and reuses its cwd and
   environment, so the model sees a stable shell identity across calls.
4. A command tries to reach the network in a policy that denies it. The
   sandbox rejects the connection (fail-closed) and the tool reports the
   failure rather than silently succeeding.

## In scope

- PTY-backed process spawning with the shell as session leader and process
  group; combined output capture with a bounded buffer.
- Timeout enforcement and cancellation grace: SIGTERM to the process group,
  escalation to SIGKILL after the grace period; exit status including signal
  termination.
- Shell snapshot: resolve shell from `SandboxConfig.shell_path`, then
  `$SHELL`, then platform default (`bash`); capture cwd and environment at
  bootstrap and reuse across calls.
- Network sandbox: explicit allow/deny derived from `Policy.allow_network`
  on both macOS (seatbelt) and Linux (bwrap `--share-net` / unshared netns).
- Wire the built-in `bash` tool through the sandbox runner with
  timeout argument and runtime cancellation token.
- Long-lived PTY processes: a process manager owned by the workspace tool
  provider, with `start` (optional cwd/env overrides), `write_stdin`, read of
  accumulated output since the last read, group `stop`, and `list`.
- Environment capability discovery: usable shell resolution (configured →
  `$SHELL` → candidates), PATH normalization/dedup, and probing for common
  tools, exposed to the model through an `environment` tool.
- Client surface: hostd `process.list` (`/ps`) returns the live process set —
  id, pid, command, cwd, exit state — and `process.stop` (`/kill <id>`)
  terminates one, mirroring codex-rs `backgroundTerminals/list` and
  `backgroundTerminals/terminate`.

## Out of scope

- Network proxy support (`network_proxy`) and per-command network decisions.
- Windows sandbox backends.
- Per-command PATH canonicalization against the policy allowlist (the static
  `validate_command` fast-fail remains; the OS sandbox is the boundary).
- Provider-level output formatting; the runner returns bounded raw output and
  the tool layer truncates.

## Behavior and states

### Spawn lifecycle

1. The runner resolves the shell snapshot (shell path, cwd, environment).
2. It allocates a PTY pair, spawns `shell -c <command>` in a new session
   (`setsid`), making the shell the process-group leader, with stdin/out/err
   connected to the PTY.
3. Output from the master side is read and combined (stdout+stderr ordering
   preserved) into a bounded buffer; the read loop stops when the child exits
   or the buffer cap is reached (with a truncation marker).
4. The wait terminates when the child exits, the timeout expires, or the
   cancellation token fires — whichever comes first.

### Termination semantics

- **Normal exit**: the result carries the exit code and full captured output.
- **Signal death**: the result carries the signal number, no fake exit code.
- **Timeout**: the runner sends SIGTERM to the process group, waits the grace
  period (default 2s, configurable), escalates to SIGKILL if still alive, and
  returns a timeout result with captured output.
- **Cancellation**: same escalation path; the result is marked cancelled and
  the tool layer commits a bounded error result so the transcript stays
  complete and replayable.

### Shell snapshot

- Resolved once per provider bootstrap: `sandbox.shell_path` →
  `$SHELL` → `"bash"` (platform default).
- Captures the bootstrap cwd and the current environment (with
  `DYLD_*`/`LD_*` loader variables removed when wrapping with seatbelt).
- Each `bash` call defaults to the snapshot cwd; a `cwd` argument is a
  follow-on (snapshot cwd is authoritative in slice 1).

### Network policy

- macOS: the seatbelt policy is deny-by-default; when `allow_network` is
  false, no `network-*` allowance is emitted (denied); when true, outbound
  (and inbound on localhost) network operations are allowed.
- Linux: bwrap always runs in an unshared network namespace; when
  `allow_network` is false, the namespace is left unshared (denied); when
  true, `--share-net` joins the host namespace.
- A policy that requests network on an unsupported backend is an explicit
  configuration error, not a silent allow.

## Acceptance criteria

- [ ] A `bash` call runs under the sandbox runner: with the sandbox enabled,
      a command writing outside the policy's writable roots fails closed
      rather than touching the filesystem.
- [ ] The process is a session-leading process group; killing the parent
      shell (timeout or cancellation) also terminates its child processes.
- [ ] A timeout terminates a sleeping command and returns a bounded timeout
      result, not a hung tool call.
- [ ] Cancelling a run aborts an in-flight `bash` call and commits a bounded
      result (existing F-06 contract preserved).
- [ ] A command terminated by a signal reports the signal, not a fabricated
      exit code.
- [ ] The shell resolves from `[sandbox] shell_path`, then `$SHELL`, then the
      platform default; the resolved path is reported in tool metadata.
- [ ] With `allowNetwork: false`, a network probe (e.g. `curl localhost`)
      fails inside the sandbox on macOS; with `allowNetwork: true`, the same
      probe succeeds.
- [ ] Linux bwrap honors `allowNetwork: true` via `--share-net` instead of
      erroring out.
- [ ] The existing file-policy tests and tool-batch tests still pass
      (differential regression).
- [ ] A `process start` keeps the command running across tool calls: output
      accumulates and is readable incrementally, stdin writes reach the
      process, and `stop` terminates the whole process group.
- [ ] Unknown process ids produce a bounded error, and a process-list call
      reflects current live processes.
- [ ] The environment tool reports the resolved shell, OS/arch, cwd, PATH,
      and detected tools without exposing credentials.
- [ ] Shell resolution falls back to a usable shell when the configured
      shell is unavailable.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where does process execution live? | `piko-sandbox` runner, called by orchd's `bash` tool | Keeps the fail-closed boundary in one crate; hostd stays authoritative for state, not processes |
| PTY or pipes? | PTY with the shell as session leader | Interactive tools work; process-group signal cascades become possible |
| Timeout tool vs runner deadline | Runner-owned deadline with SIGTERM→SIGKILL grace | No dependency on external `timeout`; works on macOS where GNU `timeout` is absent |
| Shell identity | `shell_path` → `$SHELL` → platform default, snapshotted at bootstrap | Predictable environment for the model; single resolution point |
| Network policy | Deny by default; `allowNetwork: true` permits | Fail-closed matches F-06 and the seatbelt base policy |
| Environment selection | Deferred to follow-on slice | Entry slice keeps env = snapshot; no silent PATH rewriting |

## Open questions

1. Should `bash` tools opt into parallel execution later? codex-rs allows
   it; piko defers until long-lived shell sessions are first-class
   (inherited from F-06).
2. Should each `bash` call update the snapshot cwd to the process's final
   working directory (as codex-rs does), or keep the bootstrap cwd? Slice 1
   keeps bootstrap cwd; a follow-on slice can adopt cwd tracking.

## Verification

- [V-08: F-08 exec-sandboxing acceptance evidence](../verification/V-08-exec-sandboxing.md)

## Reference evidence

- codex-rs `core/src/spawn.rs` — PTY process spawning, session leadership.
- codex-rs `core/src/shell.rs`, `core/src/shell_snapshot.rs` — shell
  resolution and snapshot reuse.
- codex-rs `core/src/sandboxing/`, `core/src/network_proxy/` — fail-closed
  platform sandboxes and network decision points.
- piko `packages/sandbox/src/runner.rs` — existing seatbelt/bwrap runner
  (slice 1 moves it behind a PTY process-group API).
- piko `packages/orchd/src/adapters/tools/workspace_handlers.rs` — current
  unsandboxed `bash` handler being replaced.
