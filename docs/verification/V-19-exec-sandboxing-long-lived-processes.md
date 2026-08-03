# V-19: F-08 slice 2 (long-lived processes + environment selection) acceptance evidence

> Date: 2026-08-03
> Fixture: `piko-sandbox` process-manager and environment-discovery unit
> tests (`exec/process.rs`, `exec/env.rs`), `piko-orchd` process-tool tests
> (`process_handlers.rs`), full workspace suite
> Environment: macOS (arm64), `cargo test -p piko-sandbox` (26 tests,
> seatbelt wrapper tests run unsandboxed), `cargo test -p piko-orchd`
> (75 tests), `cargo test --workspace` (network-capable), `cargo clippy
> --workspace --all-targets -- -D warnings`, `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-sandbox exec::process
cargo test -p piko-sandbox exec::env
cargo test -p piko-orchd --lib adapters::tools::process_handlers
cargo test -p piko-orchd --lib adapters::tools::shell_handlers
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Result

All F-08 slice 2 acceptance criteria pass:

- **Long-lived process lifecycle**: `output_accumulates_for_incremental_reads`
  starts `echo one; sleep 0.3; echo two; …` and reads the lines as separate
  chunks over time (output accumulates between reads); `write_stdin_feeds_the_process`
  round-trips `cat` (write `hello-piko\n` → read echoes it back);
  `stop_terminates_the_process_group` starts `echo $$; sleep 30 & wait`,
  reads the shell pid, stops, and verifies `kill -0 -<pgid>` fails — the
  whole group is gone (SIGTERM 15 or SIGKILL 9 escalation, both accepted);
  `list_and_get_round_trip` covers manager bookkeeping and unknown ids.
- **Tool layer round trip**: `start_write_read_stop_round_trip` drives
  `process start` → `write` (14 bytes incl. newline) → `read` (echoed
  output) → `stop` → `list` empty through `execute_process_tool`;
  `unknown_process_is_reported` returns a bounded `unknown_process` error;
  `policy_violation_blocks_start` rejects a command outside the allowlist
  before any process starts.
- **Environment selection**: `parse_path_dedupes_and_preserves_order` keeps
  first-seen order; `resolve_shell_prefers_configured_when_usable` /
  `resolve_shell_falls_back_to_bash` cover the configured → `$SHELL` →
  candidates chain with usability validation; `profile_exposes_constants_and_tools`
  checks OS/arch/PATH/tools structure. `ShellSnapshot::capture` now derives
  shell/cwd/PATH from the profile and guarantees `TERM`.
- **Provider integration**: `WorkspaceToolProvider` owns the
  `Arc<ProcessManager>` and `EnvironmentProfile`, routes `bash`/`process`/
  `environment`/file tools to their handlers, and drops kill all live groups.
  The workspace catalog now exposes six tools (read, bash, edit, write,
  process, environment).
- **Regression**: slice-1 bash tests (exit/signal/timeout/cancel/snapshot)
  still pass after the handler split; `cargo test --workspace` green;
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all` clean.
