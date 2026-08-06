# F-10: Multi-agent v2 collaboration tools

> Status: implemented (F-10/D-10/V-10)
> Priority: P1
> Source evidence: codex-rs `multi_agents_v2/*` (followup_task, interrupt_agent,
> list_agents, wait_agent); digest Block I (`docs/codex-agent-core-digest.md`)

## Summary

Parent agents supervise children with four v2 collaboration tools:
`followup_task` (send a follow-up task and trigger a turn if the agent is
idle), `interrupt_agent` (stop the agent's current turn while keeping the
agent usable), `list_agents` (enumerate the live agent tree), and `wait_agent`
(block until a mailbox update arrives or a timeout elapses).

## Problem

The existing piko multi-agent surface (spawn_agent, spawn_agent_detached,
send_agent_message, get_agent_status, collect_agent_reports, close_agent,
reopen_agent) creates children, messages them, and reads their reports, but a
supervising parent cannot steer them:

- `send_agent_message` auto-steers an active run instead of expressing a
  deterministic "follow up when you are done" turn.
- There is no way to interrupt a runaway or wrong-direction child turn.
- There is no tool view of the live agent tree.
- Waiting for a detached report requires polling `collect_agent_reports`; a
  parent cannot block on the next mailbox update.

## User journeys

1. A parent spawns a detached child, calls `wait_agent` with a timeout, the
   child finishes and its report lands in the parent inbox, and `wait_agent`
   returns the event plus the current agent statuses.
2. A parent calls `followup_task` on an idle child: a new turn starts. On a
   busy child: the task is durably queued and runs after the current turn.
3. A child run is stuck. The parent calls `interrupt_agent`, learns the
   previous activity, the run is cancelled, and the child accepts a new
   follow-up task.
4. A parent calls `list_agents` and sees the whole session agent tree ordered
   by depth, with lifecycle, activity, and unread report counts.

## In scope

- The four v2 tool surfaces: `followup_task`, `interrupt_agent`,
  `list_agents`, `wait_agent`.
- A session-scoped, best-effort mailbox notification lane with three event
  kinds: `InboxReport` (report delivered to an inbox), `RunFinished` (run
  terminal committed), `InputQueued` (follow-up durably queued).
- `wait_agent` with a mandatory timeout and an optional single-agent filter;
  it is observational and never consumes or mutates durable state.
- Parent-child authorization for follow-up and interrupt, consistent with the
  existing `authorize_input` policy.

## Out of scope

- Agent role permission layers — landed separately in F-19/D-22/V-22.
- Inter-agent completion fragments — landed separately in F-20/D-25/V-25.
- Spawn and report-collection surfaces — already landed in the v1 slice.
- New delivery modes beyond the existing `AgentInputDelivery` semantics.
- Hostd/UI surfaces that consume mailbox events (the lane is orchd-internal
  this slice).

## Behavior and states

### followup_task

`followup_task(agent_instance_id, message)` maps to `SendAgentInputRequest`
with `delivery: FollowUp`:

- Idle target: a new run starts immediately; receipt disposition `accepted`.
- Busy target: the input is durably queued (`InputQueued` commit, bounded by
  the F-01 fixed cap); receipt disposition `queued`.
- Closed/terminated target: error; nothing is written.

### interrupt_agent

`interrupt_agent(agent_instance_id)` reads the target snapshot, then cancels
the active run:

- Active run: cancelled; result includes `previous_activity: running` and
  `accepted: true`; the agent stays open and usable.
- Idle target: benign no-op result `accepted: false` with the current
  activity, not an error.
- Unknown target: error.

### list_agents

`list_agents()` returns every live session agent, depth-sorted (parents before
children), with identity, lifecycle, activity, unread report count, and latest
report summary (when present).

### wait_agent

`wait_agent(timeout_ms, agent_instance_id?)` subscribes at call time (events
that happened before the call are not replayed) and waits for the first
matching mailbox event:

- Matching event before the timeout: returns `timed_out: false` with the
  event and a fresh tree-sorted snapshot list.
- No matching event before the timeout: returns `timed_out: true` with the
  snapshot list.
- Broadcast lag: dropped events are skipped and waiting continues.
- Parent-turn cancellation: the wait aborts through the existing tool
  cancellation path.

`wait_agent` performs no durable writes; inbox items remain unread and must be
consumed with `collect_agent_reports`.

## Acceptance criteria

- [x] `followup_task` on an idle child starts a run and returns an `accepted`
      disposition; on a busy child it returns `queued` and an
      `InputQueued` durable command is committed.
- [x] `interrupt_agent` on a running child cancels the run, reports
      `previous_activity: running`, and the child accepts a later follow-up.
- [x] `interrupt_agent` on an idle child returns `accepted: false` without an
      error and without changing lifecycle.
- [x] `list_agents` returns every live agent with parents ordered before
      children and depth matching parent-child edges.
- [x] `wait_agent` returns `timed_out: false` with the child's `RunFinished`
      or `InboxReport` event before `timeout_ms` when a detached child
      reports.
- [x] `wait_agent` with no matching events returns `timed_out: true` around
      `timeout_ms` and leaves inbox items unconsumed.
- [x] `wait_agent` with an `agent_instance_id` filter ignores events from
      other agents.
- [x] Every mailbox event is published only after the corresponding durable
      commit succeeds (durability before visibility).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Does `wait_agent` require a timeout? | Yes, `timeout_ms` is required | Tool executions must be bounded; polling parents can retry. |
| Does `wait_agent` consume reports? | No | Consumption stays explicit via `collect_agent_reports`; waiting must be idempotent and retry-safe. |
| Are mailbox events replayed to late subscribers? | No, best-effort broadcast | Waiters subscribe before waiting; replay would add durability work without a consumer. |
| Which event kinds exist? | `InboxReport`, `RunFinished`, `InputQueued` | These are the three "mailbox updates" a supervisor needs (report delivery, final status, queued input). |
| `interrupt_agent` on idle: error or benign result? | Benign `accepted: false` | Reduces LLM-visible failures for a race that is otherwise normal. |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `followup_task` | kept (adapted) | Maps to existing `FollowUp` delivery: starts a run when idle, queues when busy. |
| `interrupt_agent` | kept | Uses `AgentRuntime::cancel_agent_run` plus a pre-cancel snapshot; returns previous activity. |
| `list_agents` | kept | Surfaces the existing `AgentRuntime::list_agents` depth-sorted tree as a tool. |
| `wait_agent` | kept (adapted) | Piko waits on a mailbox notification lane instead of status messages; mandatory timeout; no durable consumption. |
| Agent roles / inter-agent prompt fragments | out of this slice | Permission-profile roles landed in F-19; completion fragments landed in F-20. Optional role prompt/model layers remain deferred. |

## Open questions

1. Should `wait_agent` ever be surfaced to hostd for UI-side waits? Deferred
   until a UI consumer exists.

## Reference evidence

- codex-rs: `multi_agents_v2/*` handlers and their acceptance fixtures
  (digest Block I).
- piko: `packages/orchd/src/adapters/tools/multi_agent_provider.rs`,
  `packages/orchd/src/runtime/agent/{mod.rs,scope.rs,actor/}`, F-01/D-01
  follow-up queue semantics.
