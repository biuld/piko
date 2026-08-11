# D-41: Provider-pluggable billing

> Status: accepted
> Implements: [F-29](../features/F-29-provider-pluggable-billing.md)
> Decisions: [ADR-014](../decisions/ADR-014-registered-billing-policies.md)

## Goal

Replace the closed `TokenPricing` calculator with a provider-extensible billing
pipeline while preserving F-28 prices and user-visible totals.

## Constraints and non-goals

- `piko-llmd` owns usage normalization and estimation.
- `hostd` remains authoritative for durable session cost.
- Catalog data remains local and authoritative.
- Plugins are linked Rust implementations; runtime code loading is excluded.
- No compatibility decoder is retained for the prior fixed-component ledger.

## Proposed design

Each resolved target carries an optional `BillingPlan` containing an adapter
ID, policy ID, currency/basis labels, and opaque JSON policy configuration.
The TOML loader defaults existing entries to `semantic_tokens` and
`token_tiered`, converts their fields to policy configuration, and validates
the plan using the standard registry.

The runtime pipeline is:

```text
semantic Usage + normalized units
  -> UsageAdapterRegistry[adapter_id]
  -> BillableUsage { metric_name: quantity }
  -> PricingPolicyRegistry[policy_id]
  -> UsageCost { currency/basis, component_name: amount, total }
```

`UsageAdapter` receives gateway context and semantic usage, so provider code can
derive request-level units and transform normalized protocol units.
`PricingPolicy` owns configuration validation and estimation. `BillingRegistry`
supports registration and dispatch; duplicate IDs are rejected.

The standard adapter emits `input_tokens`, `cached_input_tokens`,
`cache_write_tokens`, and `output_tokens`, then copies additional normalized
usage units. The standard token policy implements F-28 rates and threshold
multipliers. Its monetary components use the same stable names.

`Usage` carries an open `units` map for protocol adapters to preserve future
normalized quantities such as `input_image`, `audio_output_second`, or
`search_call`. `UsageCostEntry` carries an open `components` map. Session
accumulation merges component values by key and recomputes the entry total.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Open normalized usage-unit and monetary-component maps |
| `piko-llmd` | Billing plan, registry contracts, standard adapter/policy, catalog validation, middleware dispatch |
| `piko-hostd` | No policy logic; persists and accumulates the new ledger shape |
| `piko-tui` | Continues formatting ledger totals and basis markers |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

Static catalog plan errors reject the manifest. Runtime adapter/policy errors
are reported through tracing and leave the event's ledger empty. Cancellation
has no special billing state because estimates are derived from usage events.

## Verification

- Unit tests for registry registration, dispatch, validation, arbitrary units,
  standard token prices, tiers, and ledger accumulation.
- Catalog tests for OpenAI and DeepSeek plan selection.
- Workspace tests prove host persistence and TUI totals remain valid.

## Alternatives considered

- A growing tagged enum was rejected because every provider-specific policy
  would modify the generic engine.
- Untyped callbacks stored directly in catalogs were rejected because manifests
  need cloneable, inspectable plan data and early validation.
- Dynamic plugin loading was rejected as unnecessary trust and ABI complexity.

## Rollout

1. Open protocol usage and ledger component dimensions.
2. Add billing contracts, registry, and standard implementations.
3. Carry plans through provider target resolution and dispatch middleware.
4. Migrate catalogs/tests and record verification.
