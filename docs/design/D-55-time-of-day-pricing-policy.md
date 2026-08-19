# D-55: Time-of-day pricing policy

> Status: accepted
> Implements: [F-39](../features/F-39-time-of-day-pricing.md)
> Foundation: [F-29/D-41](D-41-provider-pluggable-billing.md)

## Goal

Let a provider catalog select token rates by the wall-clock time of the
request, and update DeepSeek V4 to its official peak/off-peak CNY schedule
without changing the ledger or middleware contracts.

## Constraints and non-goals

- `piko-llmd` owns usage normalization and estimation (F-29).
- The local catalog remains authoritative; no live price discovery (ADR-012).
- No new dependencies: `chrono` is already a `piko-llmd` dependency.
- IANA timezone names are out of scope; fixed UTC offsets only.
- `token_tiered` behavior must remain byte-for-byte unchanged for existing
  OpenAI/DeepSeek-like entries that do not use windows.

## Design

### Request timestamp

`BillingContext` gains an owned request timestamp:

```rust
pub struct BillingContext<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub api_surface: &'a str,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}
```

The cost middleware fills it with `chrono::Utc::now()` when an inference usage
event arrives. Existing adapters and policies that ignore the context are
unaffected apart from the constructor call sites.

### Shared schedule estimation

The component math in `TokenTieredPolicy::estimate` (uncached input, cached
input, cache write, output, tier multipliers) is extracted into a crate-level
helper `estimate_standard(currency, basis, schedule, usage)`. `time_of_day`
reuses it, so window rates carry the same component semantics and token tiers.

### `time_of_day` policy

New module `billing/time_of_day.rs` registers policy ID `time_of_day` in the
standard registry. Its configuration shape:

```toml
policy = "time_of_day"

[configuration]
utc_offset = "+08:00"

[configuration.default]
input_per_million = 1.5
cached_input_per_million = 0.05
output_per_million = 4.5

[[configuration.windows]]
start = "09:00"
end = "12:00"

[configuration.windows.rates]
input_per_million = 3.0
cached_input_per_million = 0.10
output_per_million = 9.0
```

Rust model:

```rust
struct TimeOfDayPricing {
    utc_offset: FixedOffset,
    default: StandardTokenPricing,
    windows: Vec<TimeWindowPricing>,
}

struct TimeWindowPricing {
    start: NaiveTime,
    end: NaiveTime,
    rates: StandardTokenPricing,
}
```

Estimation:

1. Convert `context.occurred_at` to the configured `FixedOffset` and take its
   `NaiveTime`.
2. Select the first window whose half-open range contains that time; a window
   with `start > end` spans midnight (`time >= start || time < end`).
3. No match selects `default`.
4. Run the shared standard estimation with the selected schedule.

Validation (fail closed at catalog load):

- `utc_offset` parses as an ISO-8601 offset.
- All rates are finite and non-negative; tier multipliers are positive
  (same rules as `token_tiered`).
- `start != end` for every window.
- No two windows overlap, so first-match selection is unambiguous.

### Catalog update

`packages/llmd/resources/models/deepseek.toml` switches both V4 models to
`policy = "time_of_day"` with `utc_offset = "+08:00"`, the official off-peak
schedule as `default`, and two peak windows (09:00-12:00, 14:00-18:00) with the
official peak rates:

| Model | Default (off-peak) | Peak windows |
|---|---|---|
| deepseek-v4-flash | 1.5 / 0.05 / 4.5 CNY per 1M | 3.0 / 0.10 / 9.0 |
| deepseek-v4-pro | 4.5 / 0.15 / 13.5 CNY per 1M | 9.0 / 0.30 / 27.0 |

Rates are input (cache miss) / cached input / output per million tokens,
consistent with the current catalog fields.

## Test matrix

- `time_of_day` picks the peak schedule at Beijing 10:30 and 15:00, and the
  default at Beijing 13:00.
- Boundaries are half-open: Beijing 09:00 is peak; Beijing 12:00 is off-peak.
- A midnight-crossing window (22:00-06:00) matches both 23:00 and 02:00.
- Validation rejects `start == end`, overlapping windows, and a malformed
  offset.
- The loader test for the DeepSeek fixture asserts the new policy ID and both
  rate sets.
- Existing `token_tiered` tests (OpenAI tiers, cache write, OAuth basis)
  stay unchanged and green.

## Rollout

This is catalog data plus a registered policy; no ledger schema, wire DTO, or
durable model changes. Existing sessions replay usage events through the same
middleware and only the estimated amounts change.
