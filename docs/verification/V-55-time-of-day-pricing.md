# V-55: Time-of-day pricing verification

> Status: verified
> Feature: [F-39](../features/F-39-time-of-day-pricing.md)
> Design: [D-55](../design/D-55-time-of-day-pricing-policy.md)
> Environment: Rust workspace tests on macOS

## Evidence

- Official DeepSeek Chinese Models & Pricing page retrieved 2026-08-20:
  V4 Flash and V4 Pro cache-hit, cache-miss, and output rates for Beijing peak
  (09:00-12:00, 14:00-18:00) and off-peak hours, effective 2026-08-16.
- `time_of_day` policy unit tests:
  - Beijing 10:30 and 15:00 select the peak schedule; Beijing 13:00 selects the
    default (off-peak) schedule.
  - Boundaries are half-open: Beijing 09:00 is peak, Beijing 12:00 is off-peak.
  - A weekday-restricted (Monday-Friday) peak window applies on Thursday and
    Friday but falls back to the off-peak default on Saturday and Sunday.
  - A midnight-crossing window (22:00-06:00) matches Beijing 23:00 and 02:00
    but not 18:00.
  - Validation rejects a malformed UTC offset, `start == end` windows, and
    overlapping windows, plus weekday lists outside `1..=7` or with duplicates.
    Weekday windows restricted to disjoint day sets are accepted.
- Provider loader tests assert the DeepSeek fixture resolves `time_of_day`
  with `utc_offset = "+08:00"`, the official off-peak defaults, and two
  weekday-restricted peak windows with the official peak rates for both V4
  Flash and V4 Pro.
- Existing `token_tiered` tests (OpenAI tiers, cache write, OAuth
  API-equivalent basis) remain unchanged and pass.
- `cargo test -p piko-llmd` passes (integration tests run with localhost
  binding enabled).
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo fmt --all` applied.

## Result

F-39 acceptance criteria are satisfied: estimates select the rate active at
request time (including the weekday-only peak), invalid window configurations
fail closed at catalog load, and the DeepSeek V4 catalog reflects the official
CNY peak/off-peak schedule with peak restricted to weekdays without changing
the ledger or middleware contracts.
