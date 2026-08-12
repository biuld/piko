# D-44: Session bookkeeping

> Status: implemented
> Implements: [F-32](../features/F-32-session-bookkeeping.md)

## Goal

Give hostd a single domain owner for incurred usage/cost projection and
session-tree occupancy, and make compaction consume the F-04 estimator so
auto-compact and orchd preflight cannot diverge on formula.

## Constraints and non-goals

- No new crate. hostd already depends on orchd; the estimator is re-exported
  from `piko-orchd` as a narrow public API.
- `piko-protocol` stays DTO-only. `Usage::accumulate` / `context_fill` remain
  the only protocol-side helpers.
- Journal usage facts stay the sole durable incurred ledger (D-29 / D-43).
- Compaction policy, rewrite, and summarization stay in F-05.
- Live step preflight stays in orchd. It shares the estimator, not the host
  tree snapshot.
- Chrome `used` stays last-step provider fill.

## Proposed design

### 1. orchd public estimator

`piko-orchd` exposes `transcript::{message_tokens, text_tokens, estimate_messages}`
from the existing F-04 domain. hostd must not reach into `orchd::domain`.

### 2. `hostd/domain/bookkeeping`

| File | Responsibility |
|---|---|
| `ledger.rs` | `SessionState` incurred accumulation and `/usage` row projection |
| `occupancy.rs` | F-04 occupancy of a `SessionTreeEntry` slice |
| `projection.rs` | Host-authored `UsageEvent` chrome |

Data flow:

```text
llmd Usage → orchd assistant.usage → journal
                                   → bookkeeping.account_step_usage
                                        ├─ session / turn / agent ledger
                                        └─ UsageEvent.cumulative

session tree → bookkeeping.estimate_context_tokens (F-04)
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

`domain/compaction` deletes the `ceil(chars / 4)` estimator and re-exports
bookkeeping occupancy. `find_cut_point` walks `estimate_entry_tokens` so the
keep-recent tail uses the same per-entry basis as the trigger.

### 5. Session state

`SessionState` still stores `cumulative_usage` and `agent_usage`. The
mutation methods move to `bookkeeping/ledger.rs` so the session type stays a
holder, not the accounting owner.

## Package impact

| Package | Change |
|---|---|
| `piko-orchd` | Public `transcript` estimator re-export |
| `piko-hostd` | `domain/bookkeeping`; compaction consumes it; session usage impls move |

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
- **hostd import `orchd::domain::transcript`.** Rejected: domain stays crate
  private; the public `transcript` module is the contract.
- **Switch chrome `used` to occupancy.** Rejected for this slice: that
  changes the user-visible meter from last-call fill to a conservative tree
  estimate.

## Rollout

1. Re-export the orchd estimator.
2. Add hostd bookkeeping and move the incurred ledger.
3. Point compaction occupancy at bookkeeping.
4. Keep chrome semantics; route it through bookkeeping projection.
