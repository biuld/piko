# V-64: Agent control plane — lifecycle and control evidence

> Date: 2026-09-01
> Status: complete. Sufficient to mark [F-51](../features/F-51-agent-control-plane.md)
> and [D-68](../design/D-68-agent-control-plane.md) implemented.
> Feature: [F-51](../features/F-51-agent-control-plane.md)
> Design: [D-68](../design/D-68-agent-control-plane.md)
> Environment: Rust package tests on the development host

## Reproduction

```text
cargo fmt --all -- --check
cargo test -p piko-hostd --test work_snapshot_interrupt --test work_snapshot_pending_action
cargo test -p piko-hostd --test session_storage -- state_snapshot_hydrates two_clients_restart
cargo test -p piko-hostd --test session_store -- interrupt_during_pending_action applied_steer interrupt_requested_then_recovery duplicate_cancel
cargo test -p piko-orchd --test agent_runtime -- steer_then_root follow_up_while cancel_before cancel_after interrupt_idle
cargo test -p piko-session-store --test work_crash --test work_proptest
cargo test -p piko-tui -- foreground_tests queue_tests
cargo test -p piko-e2e --test control -- --test-threads=1
cargo clippy -p piko-hostd -p piko-orchd -p piko-session-store -p piko-tui -p piko-e2e --all-targets -- -D warnings
```

Desktop remains out of scope. Two-HostServer tests model restart / a second
client on one journal (not two live stdio connections to one process).
Connected-client **push** is proven on the command/event stream without a
later `StateSnapshot` / `SessionOpen`.

## Result

- Connected-client push of `SessionReconciled` after interrupt and
  pending-action request/resolve passed (`work_snapshot_interrupt`,
  `work_snapshot_pending_action`). Idle interrupt does not push.
- TUI foreground is snapshot-only; last-in dequeue cancels
  `queued_inputs.last().input_id`.
- Named races R1–R9 passed, plus existing interrupt-first coverage
  (`cancelled_run_commits_a_durable_abort_marker`,
  `v2_interrupt_agent_cancels_running_and_keeps_agent_usable`).
- Crash inventory holes C4/C5/C6/C8/C10 and projector `proptest`
  `agent_work_projection_invariants` passed.
- Hydrate: `state_snapshot_hydrates_requires_action_and_cancelling` reads
  RequiresAction/Cancelling from `current.json` (not a push test).
- Restart: `two_clients_restart_cancel_queued_follow_up_and_reject_steer`
  — two HostServers SessionOpen the same journal; unfinished root is
  recovered; pending steers do not bind to a successor; queued follow-up
  remains cancellable by `input_id`; post-restart steer is rejected.
- Cross-process JSONL: `queue_cancellation_and_surviving_identity_rehydrate_after_restart`
  proves the same queue identity, rejected-steer, and post-restart cancellation
  contract through a restarted hostd process. The control E2E also observes
  the authoritative Cancelling push before terminal reconciliation.

## Invariants

- Clients interrupt by Session and AgentInstance; no Execution identity is
  exposed on the wire.
- Detached and user-origin work share the same AgentInput admission and
  interruption path; no synthetic Turn is required.
- Idle interrupt and terminal races are benign (`accepted: false`) and cannot
  affect a successor root.
- Steers retain the active `root_input_id` captured at admission and are applied
  to one reserved ModelStep; they cannot retarget a later root.
- Queue order and cancellation are journal facts projected by hostd. A restart
  or independent second client therefore sees the same input IDs, dispositions,
  and foreground state.
- The TUI targets the viewed AgentInstance and derives queue/steer feedback only
  from `AgentWorkSnapshot`. Dequeue addresses last-admitted `input_id`.

## Named tests

Push (command/event stream, no subsequent StateSnapshot/SessionOpen):

- `interrupt_command_stream_includes_cancelling_snapshot`
- `idle_interrupt_does_not_push_session_reconciled`
- `interrupt_with_open_pending_action_pushes_requires_action`
- `pending_action_request_and_resolve_push_on_submit_observation_stream`

TUI authority:

- `local_approval_event_does_not_change_foreground_or_activity`
- `detached_runtime_activity_does_not_masquerade_as_a_host_turn_for_steer`
- `dequeue_restores_preview_and_cancels_authoritative_input`
- `activity_does_not_replace_missing_work_snapshot`

Races:

- R1 `steer_then_root_still_running_applies_to_next_step`
- R2 `steer_then_root_recovers_cancels_pending_steer`
- R3 `steer_after_root_terminal_writes_no_input`
- R4 `follow_up_while_busy_is_pending`
- R5 `follow_up_while_idle_starts`
- R6 `cancel_before_advance`
- R7 `cancel_after_advance`
- R8 `interrupt_idle_is_unaccepted`
- R9 `interrupt_during_pending_action_keeps_requires_action_until_resolve`

Crash / properties:

- C4 `pending_steer_replay_freezes_captured_root_input_id`
- C5 `applied_steer_is_not_redelivered_after_interrupt_recovery`
- C6 `interrupt_requested_then_recovery_still_finishes_the_root`
- C8 `unknown_pending_action_resolve_is_invalid_event`
- C10 `duplicate_cancel_of_already_cancelled_input_is_idempotent`
- `agent_work_projection_invariants`

Hydrate / restart:

- `state_snapshot_hydrates_requires_action_and_cancelling`
- `two_clients_restart_cancel_queued_follow_up_and_reject_steer`
- `two_clients_reconcile_the_same_authoritative_work_projection` (queued hydrate)
- `first_reconciled_snapshot_contains_atomic_interruption_recovery`
- `queue_cancellation_and_surviving_identity_rehydrate_after_restart`
- `turn_cancel_crosses_jsonl_hostd_and_orchd_and_clears_active_state`
