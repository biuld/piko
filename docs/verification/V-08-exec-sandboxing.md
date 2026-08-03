# V-08: F-08 exec-sandboxing slice 1 acceptance evidence

> Date: 2026-08-03
> Fixture: `piko-sandbox` PTY runner unit tests (`exec.rs` /
> `exec/unix.rs`, including the seatbelt wrapper tests), `piko-orchd`
> workspace-tool tests (`workspace_handlers.rs`), full workspace suite
> Environment: macOS (arm64), `cargo test -p piko-sandbox` run both inside
> the dev sandbox (wrapper tests skip via a probe) and unsandboxed
> (seatbelt wrapper tests execute as real evidence), `cargo test --workspace`,
> `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`

## Reproduction

```bash
cargo test -p piko-sandbox            # wrapper tests probe-skip in a nested sandbox
cargo test -p piko-sandbox            # unsandboxed run: seatbelt evidence below
cargo test -p piko-orchd --lib adapters::tools::workspace_handlers
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

The `piko-sandbox` suite is 18 tests: shell resolution (3), PTY lifecycle
(6), seatbelt wrapper (3), plus the pre-existing policy ACL suite. The
`piko-orchd` suite adds 4 `bash`-tool tests that drive
`execute_workspace_tool` through the real PTY runner.

## Result

All F-08 slice 1 acceptance criteria pass:

- **PTY process-group lifecycle**: `timeout_terminates_and_reports`
  terminates `sleep 30` in ~0.4 s and reports `timed_out`; a background
  child dies with the group — `background_child_dies_with_the_group` reads
  the shell pid (`$$`) from the captured output, times out, then verifies
  `kill -0 -<pgid>` fails (whole group gone). `signal_death_is_reported`
  maps `kill -KILL $$` to `signal: 9` with no fabricated exit code.
- **Output capture**: small output, 2 000-line `seq` output, pipes
  (`seq | tr`), and combined stdout ordering all round-trip through the PTY
  master; the reader uses `AsyncFd` (a plain `tokio::fs::File` read fails on
  EAGAIN and deadlocks chatty children — found and fixed during
  implementation). cwd/env passthrough verified with `pwd` and
  `$PIKO_TEST_VAR`.
- **Shell snapshot**: `resolve` precedence is configured → `$SHELL` →
  `bash`; `capture` guarantees `PATH` and strips `DYLD_*`/`LD_*`; the
  `bash` tool reuses the snapshot cwd/env (`bash_uses_shell_snapshot_cwd_and_env`).
- **Cancellation**: a pre-cancelled token short-circuits the spawn, and
  `bash_respects_runtime_cancellation` commits a bounded `cancelled` result
  (F-06 contract preserved).
- **Timeout at the tool layer**: `bash_timeout_is_bounded_and_reported`
  returns `timedOut: true` with a `timed_out` error code.
- **Network sandbox (macOS, unsandboxed run)**: with `allowNetwork: false`,
  a socket probe reports errno `1` (EPERM) —
  `macos_network_denied_when_not_allowed` passes; with `allowNetwork: true`
  the same probe connects (`result 0`) — `macos_network_allowed_when_configured`
  passes. Loopback is exempt from seatbelt network filtering, so the probe
  uses an external address (`1.1.1.1:9`). Linux bwrap honors
  `allowNetwork: true` via `--share-net` (code path in `platform.rs`; the
  Linux test runs on bwrap-capable CI).
- **Filesystem denial (macOS)**: `macos_filesystem_denied_outside_roots`
  shows `bash: /usr/piko_seatbelt_probe.txt: Operation not permitted`,
  `write-exit=1`, and no file created. Note: seatbelt platform defaults
  (`platform_defaults.sbpl`) intentionally keep `/tmp`, `/private/tmp`,
  `/var/tmp` writable for normal tooling — the acceptance probe targets a
  read-only root (`/usr`) instead.
- **Regression**: the F-06 tool-batch registry suite and all other orchd
  tests stay green; `cargo test --workspace` green; `cargo clippy
  --workspace --all-targets -- -D warnings` and `cargo fmt --all` clean.

## Environment notes

- Inside a nested seatbelt sandbox (e.g. this dev harness), `sandbox-exec`
  cannot apply a second policy (`sandbox_apply: Operation not permitted`);
  the wrapper tests probe with a trivial policy and skip, and the unsandboxed
  run above supplies the evidence.
- The macOS `python3` binary is an `xcrun` shim; the network probe policy
  adds `/Library/Developer` read access so it can bootstrap.
