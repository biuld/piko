# V-10: Multi-agent v2 collaboration tools — verification evidence

> Status: accepted
> Implements: [F-10](../features/F-10-multi-agent.md) · [D-10](../design/D-10-multi-agent-v2-tools.md)
> Date: 2026-08-02
> Command: `cargo test -p piko-orchd --test agent_runtime` (39 passed)

## Acceptance evidence

All F-10 acceptance criteria are covered by integration tests in
`packages/orchd/tests/agent_runtime_cases/multi_agent.rs` against the
`MultiAgentToolProvider` and `AgentRuntime` with a canned-response model
gateway.

| F-10 acceptance criterion | Test | Result |
|---|---|---|
| followup_task on idle starts a run (`accepted`); on busy it queues and commits `InputQueued` | `v2_followup_task_starts_a_turn_when_idle`, `v2_followup_task_queues_while_busy_and_commits_input` | pass |
| interrupt_agent cancels a running child, reports `previous_activity: running`, child stays usable | `v2_interrupt_agent_cancels_running_and_keeps_agent_usable` | pass |
| interrupt_agent on idle is a benign `accepted: false` no-op | `v2_interrupt_agent_idle_is_benign` | pass |
| list_agents returns the depth-sorted live tree | `v2_list_agents_returns_depth_sorted_tree` | pass |
| wait_agent returns `timed_out: false` with the child's `RunFinished` event before `timeout_ms` | `v2_wait_agent_returns_on_run_finished` | pass |
| wait_agent times out around `timeout_ms` and consumes nothing | `v2_wait_agent_times_out_and_consumes_nothing` | pass |
| wait_agent filter ignores other agents' events and matches the target | `v2_wait_agent_filter_ignores_other_agents_and_matches_target` | pass |
| events publish only after their durable commit | covered by publication points after `InputQueued`/`CommitReport`/`RunTerminal` commits (D-10); existing atomicity/recovery tests still green | pass |

## Differential validation against codex-rs

The four tool names and supervision semantics (follow-up turn, interrupt with
previous status, agent tree listing, bounded wait) are distilled from digest
Block I evidence (`multi_agents_v2/*`). Piko-specific adaptations asserted
here:

- `followup_task` reuses piko's existing `FollowUp` delivery (start-when-idle,
  durable queue) instead of replicating codex-rs' queue internals.
- `wait_agent` waits on a best-effort session mailbox lane and never consumes
  inbox items; reports stay explicit via `collect_agent_reports`.
- `interrupt_agent` maps "no active run" to a benign `accepted: false` result
  rather than an error.

## Regression scope

- `cargo test -p piko-orchd --test agent_runtime` — full agent runtime suite
  (39 tests) including the pre-existing F-01 follow-up queue, cancellation,
  recovery, and atomicity cases.
- `cargo test -p piko-protocol -p piko-comms` — DTO/catalog validation
  (`catalog_is_valid` guards the new `orchd.agent.mailbox_event` contract).

## Post-landing cleanup: consolidated tool surface

After the F-10 slice landed, the v1/v2 tool surface was reviewed for overlap:

- `get_agent_status` removed — it was a single-agent subset of `list_agents`.
- `send_agent_message` dropped its `delivery` enum — follow-up semantics now
  live exclusively on `followup_task`, and the tool always uses the `Auto`
  delivery (start when idle, steer while running).
- The surface is locked to 10 tools by
  `v2_consolidated_surface_has_no_redundant_tools`, which asserts the exact
  name set and that `send_agent_message` exposes no `delivery` property.
