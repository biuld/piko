# D-40: Provider-native cost accounting

> Status: superseded by [D-41](D-41-provider-pluggable-billing.md)
> Implements: [F-28](../features/F-28-provider-native-cost-accounting.md)
> Decisions: [ADR-013](../decisions/ADR-013-provider-native-cost-ledger.md)

## Goal

Replace llmd's global USD/model-ID estimator with catalog-owned pricing that
flows through resolved targets, normalizes provider usage, and accumulates a
multi-currency session ledger without leaking provider wire formats beyond
llmd.

This document records the first provider-native implementation. D-41 replaces
its closed `TokenPricing` calculator and fixed ledger components while
preserving the F-28 behavior contract.

## Constraints and non-goals

- The local executable catalog remains authoritative; no price discovery is
  added.
- `hostd` remains authoritative for durable session usage totals.
- `llmd` owns provider usage normalization and price calculation.
- Prices are floating-point display estimates, not a financial ledger or an
  invoice reconciliation system.
- No currency conversion is performed.

## Proposed design

### Catalog schedule

Each model may declare zero or more `[[models.<id>.pricing]]` entries. An entry
selects a named API surface and carries:

- `basis`: `list_price` or `api_equivalent`;
- ISO-style uppercase `currency`;
- per-million uncached-input, cached-input, output, and optional cache-write
  prices;
- zero or more request-input threshold tiers with independent input and output
  multipliers.

A schedule may `copy_from` another surface for the same model and override its
basis. OpenAI subscription targets copy the platform schedule and use
`api_equivalent`; this avoids duplicating public rates while keeping the
meaning explicit. Loader validation rejects unknown surfaces, duplicate
schedules, missing sources, malformed currency codes, invalid rates, and
non-positive tier multipliers.

### Resolution and execution flow

1. `TomlProvider` loads schedules keyed by model and API surface.
2. Authentication-aware target resolution selects one API surface and attaches
   its optional `TokenPricing` to `ResolvedModelTarget`.
3. Hostd copies the schedule into `ModelTargetConfig`; llmd carries it into the
   concrete target and request middleware context.
4. A protocol adapter normalizes provider usage fields. DeepSeek
   `prompt_cache_hit_tokens` becomes semantic `cache_read`; prompt tokens not
   served by cache remain uncached input.
5. Cost middleware partitions input tokens into cache read, cache write, and
   uncached input, selects the highest applicable request tier, computes each
   component, and emits one `UsageCostEntry`.
6. Hostd session accounting merges entries only by `(currency, basis)`.

The calculator never looks up a provider or model ID. Adding another provider
therefore requires a catalog schedule and, only when necessary, normalization
of that provider's usage fields.

### Protocol cost ledger

`UsageCost` is a ledger of `UsageCostEntry` values. Each entry contains:

- currency and `UsageCostBasis`;
- input, output, cache-read, cache-write, and total amounts.

Accumulation merges matching currency/basis entries and appends non-matching
entries. An empty ledger means pricing unavailable. A present zero-valued
entry means a known schedule computed zero cost. The schema-v3 storage policy
does not migrate the former scalar cost shape.

### Projection and telemetry

The TUI renders native currencies and prefixes API-equivalent values with `~`.
Telemetry records generic `piko.model.cost` and `piko.turn.cost` counters with
`currency` and `basis` attributes; it no longer labels every amount USD.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Multi-entry cost ledger, currency, basis, accumulation, and legacy decoding |
| `piko-llmd` | Catalog pricing schema, target propagation, provider usage normalization, pure calculator |
| `piko-hostd` | Propagate resolved pricing and record currency/basis telemetry |
| `piko-tui` | Render native and API-equivalent session cost entries |
| `piko-client-core` | Treat an available ledger as a usage signal |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Invalid catalog pricing rejects the provider manifest at load time.
- Missing pricing is non-fatal and yields no cost entry.
- Malformed or absent provider usage fields default to the adapter's existing
  zero/omitted semantics; cost is only attached to an emitted Usage event.
- Cancellation before a completed Usage event produces no estimated cost.
- Price changes require a catalog update and apply only to newly calculated
  usage; persisted historical entries are not repriced.

## Verification

- Loader tests assert OpenAI platform/list-price versus
  subscription/API-equivalent resolution and DeepSeek CNY schedules.
- Calculator tests cover cache partitions, threshold tiers, CNY, and totals.
- Adapter tests cover DeepSeek cache-hit field normalization.
- Protocol tests cover same-key accumulation and cross-currency separation.
- TUI tests cover mixed-currency and API-equivalent formatting.

## Alternatives considered

- **Global provider/model match table:** rejected because it bypasses target
  resolution and makes custom providers, API surfaces, and auth routes
  ambiguous.
- **Convert everything to USD:** rejected because conversion needs an explicit
  exchange-rate source and timestamp and would discard the provider-native
  amount.
- **Treat OAuth as zero cost:** rejected because zero falsely means no resource
  consumption. API-equivalent cost is useful when labeled honestly.
- **One scalar cost on Usage:** rejected because sessions may switch currencies
  and estimate bases.

## Rollout

1. Add the typed cost ledger.
2. Add catalog schedules and target propagation.
3. Replace hard-coded calculation and normalize DeepSeek cache usage.
4. Update session projection, telemetry, and TUI formatting.
5. Add OpenAI GPT-5.6 and DeepSeek V4 price fixtures and acceptance evidence.
