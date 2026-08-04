# F-20: Inter-agent completion fragments

> Status: implemented (F-20/D-25/V-25)
> Priority: P1
> Source evidence: codex-rs `core/src/context/{subagent_notification,inter_agent_completion_message,inter_agent_message}.rs`,
> `core/src/session_prefix.rs`, `core/src/agent/control.rs` (completion
> forward to parent), digest Block I / C; piko F-10 multi-agent mailboxes and
> F-03/F-04 retained Context messages

## Summary

When a detached child finishes and its run report is already durable in a
parent agent’s inbox, the parent’s **next** agent run injects a model-visible,
data-only Context message that names the child, the report, the terminal
outcome, and a bounded summary. The parent model sees completed child work in
transcript history without first calling `collect_agent_reports`. Inbox
consumption stays explicit and unchanged.

## Problem

piko already delivers detached child reports to a durable parent inbox and
exposes `wait_agent` / `collect_agent_reports` for supervision, but nothing
model-visible lands in the parent’s durable transcript when a report arrives.
The parent only learns outcomes if it (1) still mid-turn and actively waits or
collects via tools, or (2) re-collects on a later turn. If the supervising
turn ends without collect, the next parent turn has **no** automatic notice
that a child finished, which forces tool-polling and weakens multi-agent
supervision that should read as a linear conversation.

## User journeys

1. The root agent spawns a detached child. The child finishes and the report
   is committed to the root inbox. The user sends another message to the root.
   The root’s new run transcript contains a Context completion fragment for
   that report (source agent id, report id, outcome, bounded summary) before
   the new user message. The inbox item remains unread until
   `collect_agent_reports`.
2. The same detached report is already in the inbox. The parent calls
   `collect_agent_reports` before any further run. The collect tool returns
   the report and marks it consumed. The parent’s next run does **not** inject
   a completion fragment for that report (tool result already made it
   model-visible in the previous run).
3. Delivery retries fail briefly and then succeed. On the first parent run
   after durable delivery, exactly one completion fragment for that
   `report_id` is present (idempotent message id / content identity).
4. A child fails or is cancelled. The completion fragment states the failed
   or cancelled outcome and a bounded error or summary so the parent can
   decide whether to follow up.

## In scope

- A retained, data-only **inter-agent completion** Context message per unread
  detached inbox report, injected at the **start of the recipient agent’s
  next run** (before that run’s user input), chained after any world-state
  Context from F-04.
- Stable identity: deterministic message id and `PromptSource` keyed by
  `report_id` so retries and recovery do not double-inject.
- Content contract: source agent instance id, report id, outcome kind
  (succeeded / failed / cancelled), and a length-bounded summary (or error
  text for failures).
- Unchanged mailbox, `wait_agent`, and `collect_agent_reports` semantics;
  injection never consumes inbox items and never starts a parent turn by itself.
- Applicable to any agent instance with an unread detached inbox (root or
  intermediate parents).

## Out of scope

- Mid-run live injection into an already-running parent execution (parent
  mid-turn continues to use `wait_agent` / `collect_agent_reports`).
- Pure status notifications without a terminal report (no RUNNING /
  PENDING-only fragments).
- Reformatting of `send_agent_message` / follow-up payloads as inter-agent
  MESSAGE/NEW_TASK envelopes (recipient input already model-visible).
- Auto-starting a parent turn when a child completes (`trigger_turn`).
- Plugin / hooks inter-agent surfaces; UI-only toast of mailbox events.
- Per-role prompt layers beyond F-19 permission profiles.

## Behavior and states

### When injection runs

At the start of an accepted recipient agent run that builds a durable
transcript chain, for each inbox item where:

1. `recipient_agent_instance_id` is this agent,
2. `consumed_at` is unset (still unread),
3. no prior Context message for that `report_id` exists in the recovered or
   in-memory transcript (idempotency check by source identity),

the runtime appends one completion Context message to the durable head chain
**before** the run’s user input (and after any world-state Context when both
are present). Order of multiple unread reports is stable (committed_at, then
report_id).

### Message shape (model-visible text)

Trusted Context (authority none). Fixed key lines, absent keys omitted:

```text
inter-agent completion:
source_agent_instance_id: <id>
report_id: <id>
outcome: succeeded|failed|cancelled
summary: <bounded text>
```

For failures, `summary` prefers the outcome error; otherwise the report
summary. Empty summaries may omit the `summary` line.

### States

| State | Behavior |
|---|---|
| Unread inbox, no prior fragment | Inject on next recipient run start |
| Unread inbox, fragment already durable | Skip (idempotent) |
| Inbox item consumed via collect | Skip injection on later runs |
| Recipient mid-run when report lands | No live inject; next idle→run start |
| Deliver report fail permanently | No inbox; no fragment |
| Parent closed/terminated | Existing lifecycle rules; no special inject |

### Failure modes

- Message commit failure during run start fails the run start the same way
  as a world-state or input commit failure (durability before activation).
- Truncation of long summaries is best-effort and deterministic (fixed max
  characters after Unicode scalar-safe cut).

## Acceptance criteria

- [x] After a detached child report is durable in a parent inbox, the parent’s
      next run commits exactly one Context message whose source is
      `agent.completion` / `report_id` and whose body names the source agent,
      report id, and outcome, before the run’s user input.
- [x] Injected fragments do not mark inbox items consumed; `collect_agent_reports`
      still returns the unread report afterward.
- [x] After successful collect, a later parent run does not inject again for
      that report_id.
- [x] Retrying run-start / recommit with the same report_id does not create a
      second durable message (stable message id).
- [x] Attached `spawn_agent` (wait for report in tool result) does not create
      an inbox-based completion fragment for that wait path.
- [x] Multiple unread reports inject in stable order at one run start.
- [x] Differential validation: shape mirrors codex-rs completion envelopes
      (task/sender/payload facts) without requiring XML markers or
      auto-triggered parent turns.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Inject when? | Next recipient run start | Avoids racing a mid-run linear transcript head; tools cover mid-turn |
| Consume on inject? | No | Keep explicit collect/wait semantics (F-10) |
| Mid-status fragments? | No | Terminal reports already carry outcomes; status noise is low value |
| MESSAGE/NEW_TASK envelopes? | Rejected this slice | Child input is already the model-visible payload |
| Auto turn on completion? | No | User/agent agency stays explicit; match piko no-stealth-turns |
| Fragment placement | Retained transcript Context, not frozen prompt block | Matches F-04 world-state retention; survives for residual turns |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Inter-agent FINAL_ANSWER / completion envelope on parent history | **kept (adapted)** | Retained `Message::Context` with piko-native line format; stable id per report |
| Inject without starting a parent turn | **kept (adapted)** | Fragment is durable on run-start of next parent work; no force-turn |
| SubagentNotification JSON status fragment | **rejected (deferred)** | Terminal report completion covers the only piko consumer; no mid-status bus |
| InterAgentMessage MESSAGE/NEW_TASK | **rejected** | `send_agent_message` / follow-up already write recipient user input |
| `trigger_turn` on communication | **rejected** | No consumer; would start turns without user or tool intent |
| Assistant-role transport for completion | **kept (adapted)** | piko uses data-only Context trust model (F-03/F-04), not assistant impersonation |

## Open questions

1. Should a later slice inject mid-run into a live parent execution when the
   actor is idle between model steps? Deferred until a measured need appears.
2. Should completion fragments participate in compaction budget weighting
   differently from user text? Default: same as other Context messages.

## Reference evidence

- codex-rs: `subagent_notification.rs`, `inter_agent_completion_message.rs`,
  `session_prefix.rs`, `agent/control.rs` (forward completion), multi-agent
  tests for parent history notifications
- piko: F-10/D-10 mailbox + inbox, F-03/F-04 retained Context injection,
  `AgentDurableCommand::CommitReport`, detached delivery
