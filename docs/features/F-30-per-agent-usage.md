# F-30: Per-agent usage

> Status: implemented
> Priority: P1
> Source evidence: piko product decision; F-15 usage ledger; F-28/F-29 cost accounting

## Summary

piko exposes a read-only `/usage` surface that attributes durable token usage,
provider-native estimated cost, run count, and active execution time to every
AgentInstance in the current session. The surface replaces the generic
`/status` diagnostic modal and keeps hostd authoritative for all displayed
values.

## Problem

The existing `/status` modal mixes current-agent, session-wide, and historical
TUI-local counters. It does not help a user understand resource consumption or
act on a problem. Multi-agent sessions instead need a stable accounting view:
which agent used the tokens, time, and money.

## User journeys

1. A user runs a root agent that spawns child agents, then opens `/usage`.
2. The TUI refreshes the current session snapshot from hostd.
3. One row per AgentInstance shows runs, token categories, active time, and
   provider-native estimated cost; the viewed agent is visually marked.
4. Completed child agents remain visible after completion and session resume.
5. A session total shows the authoritative token and cost ledger without
   presenting an ambiguous aggregate duration.

## In scope

- Per-AgentInstance aggregation of input, output, cache-read, cache-write, and
  total tokens from durable assistant-message usage facts.
- Per-AgentInstance run count and active execution time from durable execution
  records.
- A host-authoritative projection in `SessionSnapshot`.
- A centered, read-only `/usage` TUI surface refreshed when opened.
- A session total for tokens and provider-native cost.
- Removal of the old `/status` queue/approval/tool/notification summary.

## Out of scope

- Provider invoice reconciliation, budgets, quotas, or enforcement.
- Currency conversion or addition across currencies and estimate bases.
- A single "session duration" derived by summing parallel agent time.
- Per-turn drill-down, charts, export, or historical cross-session reporting.
- Live sub-second counters while the usage modal remains open.

## Behavior and states

- `agent_instance_id` is the accounting identity. Reused agent specs do not
  merge distinct instances.
- Token and cost totals are rebuilt from durable assistant messages belonging
  to each AgentInstance. Messages without usage contribute zero.
- Run count includes every durable execution record for the AgentInstance.
- Active time is the sum of each execution's non-negative
  `finished_at - started_at`. A running execution contributes from
  `started_at` to the snapshot timestamp.
- Active time can overlap across agents. The session row therefore omits
  duration.
- Costs retain currency and `list_price` / `api_equivalent` basis. Unknown
  pricing renders as unavailable, not zero.
- Rows include agents with runs but no billable usage and agents known to the
  session but not yet run.
- When rows exceed the modal viewport, users can scroll through every
  AgentInstance while the session total remains visible.
- If durable execution timing cannot be read, run count and active time render
  as unavailable while token/cost facts remain visible.
- Opening `/usage` without an active session shows an empty explanatory state
  and performs no host request.
- A refresh failure preserves the last snapshot and surfaces the existing
  command error notification/status path.
- Resume reconstructs the same per-agent usage and completed active time from
  schema-v3 durable facts.

## Acceptance criteria

- [x] A session with root and child AgentInstances displays one usage row per
      instance without merging their usage.
- [x] Multiple model steps for one agent accumulate all token categories and
      cost entries exactly once.
- [x] Run count and completed active time survive session resume.
- [x] A running execution contributes elapsed time as of snapshot creation.
- [x] Parallel agent durations are not summed into or labeled as session time.
- [x] Multiple currencies and cost bases render separately.
- [x] `/usage` requests an explicit host snapshot and replaces `/status` in the
      local command surface.
- [x] Empty and zero-usage sessions render without fabricated cost values.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Accounting scope | Per AgentInstance, plus session token/cost total | AgentInstance is stable across turns and matches multi-agent ownership |
| Current-turn section | Omit | Turn state is transient and does not answer the durable accounting question |
| Duration definition | Sum durable execution wall times per agent | It is recoverable and understandable; model-only time is not a complete agent cost |
| Session duration | Omit | Parallel agent execution makes a sum misleading |
| Authority | hostd snapshot | User-visible state must not be reconstructed independently by the TUI |
| Command name | `/usage`; remove `/status` | The name states the user question and avoids implying runtime-health diagnostics |
| Cost rendering | Preserve currency and estimate basis | Required by F-28/F-29; unlike amounts cannot be validly added |

## Open questions

1. A later reporting feature may add per-run drill-down after a concrete user
   journey requires it.

## Reference evidence

- [F-15 observability and runtime debugging](F-15-observability.md)
- [F-28 provider-native cost accounting](F-28-provider-native-cost-accounting.md)
- [F-29 provider-pluggable billing](F-29-provider-pluggable-billing.md)
