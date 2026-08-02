# V-01: F-01 turn-runtime slices acceptance evidence

> Date: 2026-08-02
> Fixture: `piko-orchd` agent-runtime integration tests, `piko-orchd`
> `runtime::tasks` unit tests, `piko-hostd` `session_store` tests
> Environment: macOS, `cargo test -p piko-orchd -p piko-hostd`

## Reproduction

```bash
cargo test -p piko-orchd --test agent_runtime follow_up_queue_rejects_past_its_fixed_cap_with_overload
cargo test -p piko-orchd --test agent_runtime cancelled_run_commits_a_durable_abort_marker
cargo test -p piko-orchd --test agent_runtime startup_cancel_commits_a_durable_abort_marker
cargo test -p piko-orchd --lib runtime::tasks
cargo test -p piko-hostd --test session_store recovery_marks_accepted_execution_interrupted
```

The agent-runtime tests drive a real `AgentActor`/`ExecutionActor` with a
fake provider and collecting commit ports; the storage tests exercise the
durable manifest and agent shards directly.

## Result

All F-01 acceptance criteria for the remaining slices pass:

- **Admission cap**: follow-ups up to the fixed cap (64) queue with
  `Queued`; the next one returns `Overload`; cancelling a queued input frees
  its slot and a replacement queues.
- **Live-cancel abort marker**: cancelling a running execution commits
  exactly one durable `Message::Context` marker (`{execution_id}/abort_marker`,
  `trust: trusted`, source kind `turn_aborted`) as the last message of the
  run, and the run terminates `Cancelled`.
- **Startup-cancel abort marker**: cancelling during durable start (no model
  call) commits the same marker after the input message and terminates
  `Cancelled`.
- **Recovery abort marker**: `interrupt_incomplete_agent_executions` appends
  the stable-id marker after the last committed message (parent = previous
  head) and marks the run cancelled; a second sweep is a no-op (idempotent).
- **Typed tasks**: a spawned task with a feature-owned kind transitions
  `Running` → `Succeeded` (with a typed result) or `Failed` (with an error);
  cancel aborts in-flight work and marks `Cancelled`; `cancel_all` on session
  shutdown marks every running task cancelled. F-01 introduces no task
  taxonomy and no new durable surface — kinds and result persistence are
  owned by the consuming features (F-05/F-08/F-11).

## Invariants

- Overload is bounded and never drops queued work; cancelling a queued input
  frees its slot.
- An interrupted turn always carries exactly one durable model-visible abort
  marker with a stable id, whether the interruption is a live cancel or a
  crash, and re-recovery never duplicates it.
- Session shutdown leaves no orphaned running tasks; cancellable work is
  aborted and marked cancelled.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean and
  `cargo test --workspace` passes.
