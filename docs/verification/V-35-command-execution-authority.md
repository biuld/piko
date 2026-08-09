# V-35: Command execution authority and containment

> Date: 2026-08-09
> Fixture: workspace command tools, host approval policy, and platform sandbox
> Environment: macOS development workspace; Rust workspace tests
> Outcome: passed (`cargo test --workspace`, exit 0; strict workspace Clippy,
> zero warnings)

## Reproduction

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Focused executable fixtures:

```bash
cargo test -p piko-orchd exec_handlers
cargo test -p piko-orchd sandbox_denial_gets_one_approved_elevated_retry
cargo test -p piko-sandbox exec::process::tests
cargo test -p piko-hostd domain::permissions
```

## Result

- `exec_command` accepted a multiline program containing `cd`, environment
  assignment, command substitution, a heredoc, and a pipeline.
- Exit codes 1, 7, and 127 were returned as successful tool observations.
- A command that exceeded its yield window returned one live `session_id`;
  `write_stdin` observed the same process to completion.
- Pipe mode preserved `\n` output and PTY mode remained available explicitly.
- A workdir outside relative session roots was denied before spawn; relative
  roots could not be rebased by changing workdir.
- A typed sandbox denial requested one elevated approval and performed exactly
  one retry. Elevated calls did not recurse.
- Missing containment failed closed. The macOS nested-sandbox fallback and
  direct retry paths were removed; Linux reports a missing `bwrap` backend as
  unavailable.
- Default permissions exposed workspace read/write roots, platform scratch
  roots, restricted network, read-only `.git`/`.codex`/`.agents`, and denied
  `.piko` state.
- `bash`, `process`, the static shell parser, executable whitelist, sandbox
  enable switch, and policy-JSON loader were absent from executable routing.

## Invariants

- Shell syntax acceptance is independent of command authorization.
- Hostd authorizes requested authority; orchd owns attempts and the single
  retry budget; piko-sandbox only enforces effective permissions and process
  lifecycle.
- Approval never substitutes for containment, and restricted execution never
  silently degrades to direct execution.
- Nonzero program exits are data. Tool errors are reserved for argument,
  authorization, containment, spawn, session, and internal failures.
- Reusable approval prefixes are explicit, narrow, matched to a simple argv
  command, and bound to authority plus workdir.
