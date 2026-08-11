# ADR-013: Preserve provider-native currencies in a typed cost ledger

> Status: accepted
> Date: 2026-08-12

## Context

piko's original usage cost was one unlabeled USD-shaped scalar. Model pricing
is provider-, model-, API-surface-, and sometimes authentication-dependent.
OpenAI OAuth subscription usage has no direct per-token charge but still needs
a comparable observation value. DeepSeek publishes a Chinese price schedule
in CNY. A session can switch among these routes.

## Decision

- Price schedules are local catalog data scoped to model API surfaces.
- Usage stores a ledger keyed by native currency and estimate basis.
- `list_price` records a provider's published price for the executed API
  route. `api_equivalent` records what the usage would cost at the copied
  public API schedule and must be visibly marked as an estimate.
- Session accounting never sums different currencies or bases.
- piko performs no implicit currency conversion.
- Missing pricing is represented by no ledger entry, not zero.

## Consequences

- Provider additions normally require catalog data, not calculator code.
- Clients and telemetry must handle multiple monetary entries.
- OAuth consumption remains observable without claiming it was billed.
- DeepSeek remains denominated in CNY as requested.
- Cross-currency reporting requires a future explicit FX feature with a dated
  source; it cannot be added as an invisible display convenience.
