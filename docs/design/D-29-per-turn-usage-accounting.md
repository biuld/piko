# D-29: F-15 per-turn usage accounting

> Status: implemented
> Implements: [F-15](../features/F-15-observability.md) per-turn usage slice

## Goal

Make **hostd the sole product/session source of truth** for token/cost
accounting: model-step usages roll up into turn totals and the session
`cumulative_usage`, and OTel turn-level metrics project the same numbers.

## Constraints and non-goals

- Durable fact remains the assistant message `usage` field (written by
  orchd from llmd `GatewayEvent::Usage`). No second durable usage store.
- Step-level OTel counters (`piko.model.tokens` / cost) continue to be
  recorded by llmd at provider completion time; this slice adds **turn**
  projection from hostd, not a competing step ledger.
- Non-goals: budget policies that *act* on these totals (F-05 / future
  budget); rollout recorder; UI layouts for usage chrome (clients only need
  the protocol fields).

## Proposed design

### 1. Ledger shape

```text
llmd GatewayEvent::Usage
  → orchd assistant message.usage (per model step)
  → hostd on new MessageCommitted projection:
       turn.usage  += step   (by source_turn_id)
       session.cumulative_usage += step
  → TurnEvent::{Completed,Failed,Cancelled}.usage = turn.usage
  → telemetry.record_turn_usage(turn.usage)   // OTel projection
```

`Usage::accumulate` (protocol) is the shared add operation for tokens and
cost fields.

### 2. Hostd domain

- `TurnRecord` gains `usage: Usage` (starts empty at `start_turn`).
- `SessionState::account_step_usage(turn_id, usage)` accumulates into the
  turn (when present) and always into `cumulative_usage`.
- Idempotency: only **new** message ids (first commit projection) account;
  existing transcript-idempotent path already gates on `is_new`.

### 3. Wire

- Terminal `TurnEvent` variants carry `usage: Usage` with
  `#[serde(default)]` so older clients ignore or default to zeros.
- `TurnSnapshot.usage` is optional for live active-turn progress in
  snapshots (in-flight roll-up).
- `SessionSnapshot.cumulative_usage` unchanged.

### 4. Resume

On load, after entries are assembled, rebuild
`session.cumulative_usage` by walking assistant messages with usage. Past
turn maps are not restored (turns are process-local); historical per-turn
totals remain derivable from `source_turn_id` on messages if needed.

### 5. orchd execution report

When committing an assistant message with usage, the execution actor also
accumulates into `ExecutionState.usage` so `AgentRunReport.usage` matches
the sum of steps on that run. Hostd product ledger still trusts the
assistant messages, not the report alone.

### 6. OTel

At terminal turn lifecycle hostd records:

- `piko.turn.tokens` counter (token_type labels), from turn ledger
- `piko.turn.cost_usd` counter, from turn `usage.cost.total`

Duration/call counts stay on `record_turn` as before.

## Files

| Area | Change |
|---|---|
| `piko-protocol` | `Usage::accumulate`; terminal `TurnEvent.usage`; `TurnSnapshot.usage` |
| `hostd` domain | `TurnRecord.usage`; `account_step_usage`; rebuild on load |
| `hostd` projection | call account on new assistant commits |
| `hostd` lifecycle | emit usage on complete/fail/cancel + OTel write-through |
| `orchd` execution | accumulate step usage into run report |
| tests | hostd unit/integration for multi-step turn + resume |

## Acceptance mapping

See F-15 “Per-turn usage accounting” criteria and
[V-29](../verification/V-29-per-turn-usage-accounting.md).
