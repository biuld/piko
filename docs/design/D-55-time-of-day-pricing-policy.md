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
days = [1, 2, 3, 4, 5]

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
    days: Vec<u8>,        // ISO weekday numbers, 1=Monday..7=Sunday; empty = every day
    rates: StandardTokenPricing,
}
```

Estimation:

1. Convert `context.occurred_at` to the configured `FixedOffset` and take its
   `NaiveTime` and ISO weekday number.
2. Select the first window whose half-open range contains that time on a day
   the window allows. A non-crossing window applies when `weekday ∈ days` and
   `start <= time < end`. A window with `start > end` spans midnight: it
   matches `time >= start` on `weekday`, or `time < end` on the day after
   `weekday` (see weekday semantics below).
3. No match selects `default`.
4. Run the shared standard estimation with the selected schedule.

Validation (fail closed at catalog load):

- `utc_offset` parses as an ISO-8601 offset.
- All rates are finite and non-negative; tier multipliers are positive
  (same rules as `token_tiered`).
- `start != end` for every window.
- Every `days` entry is in `1..=7` with no duplicates; an empty list means the
  window applies every day.
- No two windows overlap, so first-match selection is unambiguous.

### Weekday semantics

An empty `days` list preserves the original every-day behavior. For a window
that crosses midnight, the weekday restriction applies to both sides: the late
`[start, 24:00)` segment matches when the local weekday is in `days`, and the
early `[00:00, end)` segment matches when the previous local weekday is in
`days`. This keeps a Monday-Friday overnight window covering, for example,
Friday 22:00 through Saturday 06:00.

Overlap validation evaluates the two windows' covered `(weekday, time)`
regions, so windows that overlap in time but are restricted to disjoint day
sets are accepted.

### Catalog update

`packages/llmd/resources/models/deepseek.toml` switches both V4 models to
`policy = "time_of_day"` with `utc_offset = "+08:00"`, the official off-peak
schedule as `default`, and two peak windows restricted to weekdays
(Monday-Friday, 09:00-12:00 and 14:00-18:00) with the official peak rates:

| Model | Default (off-peak) | Peak windows |
|---|---|---|
| deepseek-v4-flash | 1.5 / 0.05 / 4.5 CNY per 1M | 3.0 / 0.10 / 9.0 |
| deepseek-v4-pro | 4.5 / 0.15 / 13.5 CNY per 1M | 9.0 / 0.30 / 27.0 |

Rates are input (cache miss) / cached input / output per million tokens,
consistent with the current catalog fields.

## Test matrix

- `time_of_day` picks the peak schedule at Beijing 10:30 and 15:00, and the
  default at Beijing 13:00.
- A weekday-restricted window (Monday-Friday) applies its peak rates on a
  weekday (Thursday/Friday) but falls back to the default on Saturday/Sunday.
- Boundaries are half-open: Beijing 09:00 is peak; Beijing 12:00 is off-peak.
- A midnight-crossing window (22:00-06:00) matches both 23:00 and 02:00.
- Validation rejects `start == end`, overlapping windows, and a malformed
  offset, plus weekday lists outside `1..=7` or containing duplicates.
- Windows that overlap in time but are restricted to disjoint day sets are
  accepted.
- The loader test for the DeepSeek fixture asserts the new policy ID and both
  rate sets and the weekday restriction.
- Existing `token_tiered` tests (OpenAI tiers, cache write, OAuth basis)
  stay unchanged and green.

## Rollout

This is catalog data plus a registered policy; no ledger schema, wire DTO, or
durable model changes. Existing sessions replay usage events through the same
middleware and only the estimated amounts change.
