# F-39: Time-of-day pricing schedules

> Status: implemented
> Priority: P1
> Source evidence: DeepSeek Models & Pricing (retrieved 2026-08-20);
> piko product decision; F-28/F-29 foundation

## Summary

piko estimates model-call cost from provider-reported token usage and a local,
versioned price schedule. Some providers publish rates that depend on the
wall-clock time of the request. DeepSeek moved the V4 family to peak/off-peak
pricing effective 2026-08-16, where peak hours (Beijing time 09:00-12:00 and
14:00-18:00) are charged at double the off-peak rate. piko must select the rate
that was active when the request occurred so the session ledger stays honest
across the day.

## Problem

F-28/F-29 delivered a `token_tiered` policy whose rates are constant for a
request. DeepSeek's new schedule cannot be represented by a single rate set:
using the off-peak rate understates cost by 100% during peak hours, and using
the peak rate overstates cost by 100% for the rest of the day. A catalog that
silently picks one side would corrupt the user-visible cost ledger that F-28
and F-32 made authoritative.

## User journeys

1. A user calls `deepseek-v4-flash` at 15:00 Beijing time. Usage is priced with
   the peak window rates and the session displays the result in CNY.
2. The same user calls the same model at 20:00 Beijing time. Usage is priced
   with the default (off-peak) rates.
3. A provider schedule declares an invalid time window or an unsupported
   timezone expression. The catalog is rejected at load time rather than
   producing a wrong estimate.
4. A request arrives while no window matches (for example outside all declared
   windows). The default schedule applies; pricing never silently fails to
   zero.

## In scope

- A registered time-of-day token pricing policy in `piko-llmd` that selects a
  token schedule from the request's local wall-clock time.
- A request timestamp carried into billing estimation.
- Fixed UTC-offset timezone expressions in policy configuration (sufficient
  for Beijing, UTC+8 year-round).
- Half-open time windows with midnight-crossing support and validation of
  invalid or overlapping windows.
- DeepSeek V4 Flash/Pro catalog entries updated to the official 2026-08-16 CNY
  peak/off-peak schedule.

## Out of scope

- IANA timezone names or DST-aware conversion; no current provider schedule
  needs them.
- Live price discovery; the local catalog remains authoritative (ADR-012).
- Currency conversion or multiple currencies in one estimate (F-28).
- Non-token billable units (F-29 remains the extension point for those).

## Behavior and states

- A billing plan may select the `time_of_day` policy. Its configuration carries
  a fixed UTC offset, a default standard token schedule, and zero or more
  ordered windows, each with a start, an end, and its own standard token
  schedule (including optional cache-write and token tiers).
- Estimation computes local wall-clock time as request time (UTC) plus the
  configured offset, then selects the first window whose half-open range
  `[start, end)` contains that time. No match falls back to the default
  schedule.
- A window may cross midnight (`start > end`). A window with `start == end` is
  invalid, and overlapping windows are rejected during catalog validation so
  rate selection stays deterministic.
- Missing pricing plans still produce an empty cost ledger, and policy
  failures still leave cost unavailable with diagnostic telemetry (unchanged
  from F-29).

## Acceptance criteria

- [x] A `time_of_day` billing plan estimates the active window's rates for a
      given request timestamp and falls back to the default schedule when no
      window matches.
- [x] Window boundaries are half-open: 09:00 Beijing time is peak, 12:00
      Beijing time is off-peak.
- [x] Midnight-crossing windows work; invalid (`start == end`) and overlapping
      windows are rejected at catalog validation.
- [x] DeepSeek V4 Flash and Pro catalogs carry the official CNY peak/off-peak
      rates and produce correct estimates in both windows.
- [x] Existing `token_tiered` behavior (OpenAI schedules, tiers, cache write)
      remains unchanged.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| How is the timezone expressed? | Fixed UTC offset in policy configuration | Beijing is UTC+8 all year; avoids a new IANA/database dependency for the only current consumer |
| What applies when no window matches? | The default schedule | Peak is the declared exception; the rest of the day is off-peak by DeepSeek's definition |
| Can windows overlap? | No; rejected at validation | Keeps rate selection deterministic and catches catalog mistakes |
| Can a window cross midnight? | Yes | Provider schedules may legitimately span midnight |
| Which rate is authoritative? | The local catalog schedule at request time | Preserves ADR-012 and F-28's local-catalog authority |

## Fusion decisions (codex-rs)

Not derived from codex-rs; piko product direction on the F-28/F-29 billing
foundation.

## Open questions

1. A future provider may publish DST-affected schedules; if that happens, the
   policy can grow IANA-name support behind the same configuration shape.

## Reference evidence

- DeepSeek Chinese Models & Pricing page, retrieved 2026-08-20: V4 Flash and
  V4 Pro cache-hit, cache-miss, and output rates for peak (Beijing 09:00-12:00,
  14:00-18:00) and off-peak hours.
- [F-28 provider-native cost accounting](F-28-provider-native-cost-accounting.md)
- [F-29 provider-pluggable billing](F-29-provider-pluggable-billing.md)
