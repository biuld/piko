# D-44: Session bookkeeping

> Status: implemented
> Implements: [F-32](../features/F-32-session-bookkeeping.md)

## Goal

Give hostd a single domain owner for incurred usage/cost projection and
session-tree occupancy, and make compaction consume the F-04 estimator so
auto-compact and orchd preflight cannot diverge on formula.

## Constraints and non-goals

- No new crate. The host application reaches the orchd estimator through a
  host-owned port implemented at the adapter boundary; host domain remains
  runtime-independent.
- `piko-protocol` stays DTO-only. `Usage::accumulate` / `context_fill` remain
  the only protocol-side helpers.
- Journal usage facts stay the sole durable incurred ledger (D-29 / D-43).
- Compaction policy, rewrite, and summarization stay in F-05.
- Live step preflight stays in orchd. It shares the estimator, not the host
  tree snapshot.
- Chrome `used` stays last-step provider fill.

## Proposed design

### 1. Estimator port and orchd adapter

`piko-orchd` exposes `transcript::{message_tokens, text_tokens, estimate_messages}`
from the existing F-04 domain. `ports::TranscriptEstimator` is the host-owned
boundary used by application compaction. `adapters::bookkeeping` maps
`SessionTreeEntry` values to that API. Neither `hostd/domain/bookkeeping` nor
`hostd/domain/compaction` imports orchd.

### 2. `hostd/domain/bookkeeping`

| File | Responsibility |
|---|---|
| `ledger.rs` | `SessionState` incurred accumulation and `/usage` row projection |
| `occupancy.rs` | Pure occupancy value types and projection from estimated/provider facts |
| `projection.rs` | Host-authored `UsageEvent` chrome |

`adapters/bookkeeping.rs` owns session-tree estimation, and
`ports/transcript_estimator.rs` lets application compaction consume it without
reversing the application/adapter dependency.

Data flow:

```text
llmd Usage → orchd assistant.usage → journal
                                   → bookkeeping.account_step_usage
                                        ├─ session / turn / agent ledger
                                        └─ UsageEvent.cumulative

session tree → TranscriptEstimator port → orchd F-04 estimator
             → bookkeeping ContextUsageEstimate
                 └─ compact trigger / cut-point / tokens_before|after
```

### 3. Occupancy rules

- `Message` entries use `message_tokens(&message)`.
- Compaction summaries, branch summaries, and custom-message text use
  `text_tokens` plus the same +16 framing surcharge.
- Other entry kinds contribute zero.
- `estimate_tokens(text)` is the raw F-04 `text_tokens` helper for callers
  that already flattened text.

`ContextUsageEstimate` keeps its existing fields so F-05 callers do not
change shape. `ContextOccupancy` adds window, estimated used, last provider
fill, and remaining for future chrome that wants occupancy explicitly.

### 4. Compaction wiring

`domain/compaction` deletes the `ceil(chars / 4)` estimator. Application
supplies estimated totals to the trigger and injects the port operation into
`find_cut_point`, so the pure domain decision and keep-recent tail use the
same per-entry basis without importing an adapter.

### 5. Session state

`SessionState` still stores `cumulative_usage` and `agent_usage`. The
mutation methods move to `bookkeeping/ledger.rs` so the session type stays a
holder, not the accounting owner.

## Package impact

| Package | Change |
|---|---|
| `piko-orchd` | Public `transcript` estimator re-export |
| `piko-hostd` | `domain/bookkeeping`; estimator port/adapter; compaction consumes numeric estimates; session usage impls move |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

Bookkeeping is pure projection. A missing session or missing usage fact
contributes zero. Compaction still fail-closes on rewrite errors; occupancy
cannot invent cost or usage.

## Verification

- Unit tests: occupancy matches orchd `message_tokens`; `chars/4` is not used.
- Ledger tests: existing session/resume accumulation stays green.
- Compaction tests: hysteresis test updated to the F-04 rearm baseline.

## Alternatives considered

- **New `piko-bookkeeping` crate.** Rejected: one consumer for the ledger;
  extracting a crate does not change the hostd → orchd dependency.
- **Move estimator into protocol.** Rejected: protocol is DTO-only.
- **Host domain imports orchd's public `transcript` module.** Rejected: public
  visibility does not make one runtime a valid dependency of another
  runtime's domain; integration belongs behind a host port in adapters.
- **Switch chrome `used` to occupancy.** Rejected for this slice: that
  changes the user-visible meter from last-call fill to a conservative tree
  estimate.

## Rollout

1. Re-export the orchd estimator and implement the host estimator port in an adapter.
2. Add hostd bookkeeping and move the incurred ledger.
3. Pass adapter-produced estimates into compaction domain policy.
4. Keep chrome semantics; route it through bookkeeping projection.
