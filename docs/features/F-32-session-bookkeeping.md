# F-32: Session bookkeeping

> Status: implemented
> Priority: P1
> Source evidence: piko product decision; F-04 estimator; F-05 compaction
> occupancy; F-15 / F-28 / F-30 usage and cost ledgers

## Summary

hostd owns one session bookkeeping surface for incurred token usage, provider-native
estimated cost, and context occupancy. Compaction, usage chrome, and `/usage`
read that surface. Occupancy uses the same conservative estimator as the orchd
budget preflight. Incurred usage remains the journal-backed provider `Usage`
fact and is never mixed into occupancy.

## Problem

Token usage, cost, and context occupancy already exist, but they live in
different owners and disagree on the occupancy number:

1. The product ledger accumulates provider-reported `Usage` on hostd.
2. orchd estimates the dispatched transcript with `ceil(bytes / 3)` plus
   framing.
3. hostd compaction estimated flattened entry text with `ceil(chars / 4)`.
4. Usage chrome reports the last step's `input + cache_read`.

Operators and the model cannot tell which number is authoritative. Auto-compact
and fail-closed preflight can disagree about whether the window is full.

## User journeys

1. A long session approaches the model window. Auto-compact fires from the
   same occupancy estimate the next model request would use for preflight.
2. A user opens `/usage`. Session and per-agent token and cost totals still
   come from durable assistant-message usage, not from occupancy estimates.
3. A turn completes. Usage chrome still shows last-step provider fill against
   the catalog window, plus the session cumulative ledger.
4. A session resumes. Cumulative and per-agent ledgers rebuild from journal
   usage facts; occupancy is recomputed from the restored tree.

## In scope

- One hostd bookkeeping owner for session / turn / AgentInstance usage and
  cost accumulation.
- One occupancy estimate for the host-projected session tree, using the F-04
  estimator.
- Compaction trigger, cut-point, and tokens-before/after records consume that
  occupancy estimate.
- Usage chrome and session snapshot continue to project the incurred ledger
  through bookkeeping.
- Resume rebuild stays journal-derived; bookkeeping does not add a second
  durable usage store.

## Out of scope

- A new crate.
- Pricing, billing policies, or invoice reconciliation (F-28 / F-29).
- Compaction policy, rewrite, or summarization (F-05).
- Live step preflight and `get_context_remaining` inputs (orchd; same
  estimator, different transcript).
- Changing chrome `used` from last-step provider fill to estimated occupancy.
- Currency conversion or adding unlike cost entries (ADR-013).
- Budgets, quotas, or enforcement that act on the ledger.

## Behavior and states

- **Incurred ledger:** each new assistant message with `usage` adds that
  record to the session total, the matching open turn, and that turn's
  AgentInstance. Missing usage contributes zero. Different currencies and
  estimate bases stay separate.
- **Occupancy:** bookkeeping estimates each session-tree entry with the F-04
  basis (`text ≈ ceil(bytes / 3)`, serialized JSON, image framing, +16 per
  message). Compaction summaries and other non-message text pay the same
  text cost plus framing. Metadata-only entries contribute zero.
- **Chrome:** `used` remains last-step `context_fill` (`input + cache_read`).
  `cumulative` remains the incurred session ledger. Occupancy is not shown as
  billed usage.
- **Resume:** load rebuilds incurred totals from journal usage facts. Occupancy
  is derived from the restored projected tree when compaction next asks.
- **Empty session:** ledgers are empty, occupancy is zero, missing cost stays
  unavailable rather than zero.

## Acceptance criteria

- [x] Session, turn, and AgentInstance usage still accumulate exactly once per
      new assistant `usage` fact.
- [x] Occupancy of a message entry equals orchd `message_tokens` for that
      message.
- [x] Compaction trigger, cut-point, and tokens-before/after use the occupancy
      estimate, not `ceil(chars / 4)`.
- [x] Usage chrome `used` remains last-step provider fill; `cumulative` remains
      the incurred ledger.
- [x] Resume still rebuilds incurred totals from journal facts.
- [x] No second durable usage store and no new crate.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Crate boundary | hostd domain plus estimator port/adapter | The ledger has one consumer; orchd integration stays outside the pure host domain |
| Occupancy vs incurred | Separate facts | Provider usage is billed consumption; occupancy is a conservative window estimate |
| Estimator owner | orchd F-04 implementation behind a host port | Preflight already uses it; the host adapter delegates without coupling host domain to orchd |
| Chrome `used` | Last-step provider fill | That is what the last model call reported; estimated occupancy is a different question |
| Durable store | Journal usage facts only | D-29 / F-31 remain the authority |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| ContextManager token accounting | kept (adapted) | Already F-04; bookkeeping consumes the same estimator on the host tree |
| Session token/cost roll-up | kept (adapted) | Existing hostd ledger, now owned by bookkeeping |
| Single occupancy formula across compact and dispatch | kept | Closes the hostd `chars/4` drift |

## Open questions

None.

## Reference evidence

- [F-04 context-management](F-04-context-management.md)
- [F-05 compaction](F-05-compaction.md)
- [F-15 observability](F-15-observability.md)
- [F-28 provider-native cost accounting](F-28-provider-native-cost-accounting.md)
- [F-30 per-agent usage](F-30-per-agent-usage.md)
- [F-31 durable session journal](F-31-durable-session-journal.md)
