# F-28: Provider-native cost accounting

> Status: implemented
> Priority: P1
> Source evidence: OpenAI model pricing documentation; DeepSeek Models &
> Pricing and Context Caching documentation; piko product decision
>
> Billing extensibility and the open component representation are specified by
> [F-29](F-29-provider-pluggable-billing.md).

## Summary

piko estimates model-call cost from provider-reported token usage and a local,
versioned price schedule. Estimates retain the provider's native currency and
state whether they represent provider list price or an API-equivalent value.
Session totals remain meaningful when users switch providers, currencies, or
authentication routes.

## Problem

The original gateway recognized a few model IDs in one hard-coded USD match
statement. Unknown models silently appeared to cost zero, provider-specific
cache usage was lost, OAuth subscription traffic could not be observed, and
DeepSeek's RMB prices could not be represented. Adding more model IDs to that
statement would preserve the underlying ambiguity and would eventually mix
unrelated currencies in session totals.

## User journeys

1. An API-key user calls an OpenAI model. The completed usage record includes
   a USD list-price estimate split across uncached input, cache read, cache
   write, and output.
2. An OAuth user calls the same model. The session still shows its public-API
   equivalent USD cost, visibly marked as an equivalent estimate rather than
   an amount charged to the subscription.
3. A user calls DeepSeek. Cache-hit and cache-miss tokens are priced using the
   DeepSeek schedule and the session displays the result in CNY.
4. A session switches between OpenAI and DeepSeek. The UI shows separate USD
   and CNY totals and never applies an implicit exchange rate.
5. A model has no local price schedule. Token usage remains available, while
   cost is unavailable rather than reported as zero.

## In scope

- Provider/model/API-surface price schedules in the authoritative local model
  catalog.
- Provider-native currencies without automatic foreign-exchange conversion.
- List-price and API-equivalent estimate bases.
- Input, cache-read, cache-write, and output price components.
- Threshold tiers that can multiply input and output rates for one request.
- Provider usage-field normalization, including DeepSeek cache-hit tokens.
- Session accumulation grouped by currency and estimate basis.

## Out of scope

- Invoice reconciliation, account balance lookup, negotiated discounts,
  taxes, credits, batch discounts, or provider promotional pricing.
- Live pricing discovery; local catalog data remains authoritative under
  ADR-012.
- Currency conversion or a user-selected reporting currency.
- Predicting cost before a provider returns usage.

## Behavior and states

- A completed provider response first normalizes usage into input, output,
  cache-read, and cache-write token counts.
- The resolved model target supplies at most one price schedule for its API
  surface. Missing pricing produces no cost entry.
- A schedule produces one cost entry containing currency, basis, component
  amounts, and total. The total is an estimate derived from returned usage;
  it is not an invoice.
- `list_price` means the provider's published price applies to that API route.
  `api_equivalent` means the same public API schedule is used for observation,
  but the route itself is not billed token-by-token (for example OpenAI OAuth
  subscription access).
- Session accumulation merges only entries with the same currency and basis.
  Other entries remain separate.
- Clients mark API-equivalent amounts with `~`, render USD with `$`, CNY with
  `¥`, and join multiple ledger entries without summing them.

## Acceptance criteria

- [x] Price selection is resolved from provider + model + API surface, not a
      global model-ID switch.
- [x] Unknown pricing yields an empty cost ledger rather than numeric zero.
- [x] OpenAI GPT-5.6 API-key usage produces USD list-price entries.
- [x] OpenAI GPT-5.6 OAuth usage produces USD API-equivalent entries.
- [x] GPT-5.6 threshold and cache-write schedules are applied per request.
- [x] DeepSeek V4 Flash and Pro produce CNY list-price entries using distinct
      cache-hit, cache-miss, and output rates.
- [x] DeepSeek `prompt_cache_hit_tokens` is normalized to cache-read usage.
- [x] Session accumulation keeps different currencies and bases separate.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where is pricing authoritative? | The local provider model catalog, scoped to an API surface | Price applicability follows the same provider/model/auth target resolution as execution |
| Is OAuth usage costed? | Yes, as `api_equivalent` | Users need comparable session consumption, but it must not imply a subscription charge |
| What currency does DeepSeek use? | CNY from the Chinese official price schedule | Preserve the provider-native amount requested by the product; no hidden FX assumptions |
| Can currencies be added together? | No | A numeric sum without an exchange rate and timestamp is invalid |
| What does missing pricing mean? | Unavailable | Zero is a valid computed amount and must not mean unknown |
| Are estimates invoices? | No | Returned usage and public list price omit account-specific billing adjustments |

## Open questions

1. A future reporting feature may optionally convert native currencies using
   an explicit, timestamped exchange-rate source; it must preserve the native
   ledger alongside the converted view.

## Reference evidence

- OpenAI GPT-5.6 Sol, Terra, and Luna model pricing pages and latest-model
  cache/long-context pricing guidance (retrieved 2026-08-12).
- DeepSeek Chinese Models & Pricing and Token Usage pages (retrieved
  2026-08-12).
- ADR-012 local model catalog authority.
