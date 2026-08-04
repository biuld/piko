# V-29: F-15 per-turn usage accounting

> Feature: [F-15](../features/F-15-observability.md) (per-turn usage slice)
> Design: [D-29](../design/D-29-per-turn-usage-accounting.md)
> Date: 2026-08-05

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Step usage accounts into turn + session | `step_usage_accounts_into_turn_and_session` (`state_resume`) |
| Multi-step roll-up + terminal event | `multi_step_usages_roll_up_on_turn_completed` |
| Resume rebuilds cumulative from transcript | rebuild path inside multi-step test + `rebuild_cumulative_usage_from_entries` on load |
| Protocol accumulate helper | `messages::usage_tests::accumulate_sums_tokens_and_cost` |
| Turn OTel projection uses ledger | `Telemetry::record_turn_usage` on first terminal transition |

## Commands

```bash
cargo test -p piko-protocol usage
cargo test -p piko-hostd --test state_resume step_usage multi_step
cargo test -p piko-client-core
cargo test -p piko-orchd --lib
```

## Results

| Test | Result |
|---|---|
| `messages::usage_tests::accumulate_sums_tokens_and_cost` | pass |
| `step_usage_accounts_into_turn_and_session` | pass |
| `multi_step_usages_roll_up_on_turn_completed` | pass |
| `piko-client-core` suite | pass |
| `piko-orchd --lib` | pass |

## Notes

- Step-level `piko.model.tokens` remain llmd-side; turn counters are
  `piko.turn.tokens` / `piko.turn.cost_usd` from hostd’s turn ledger.
- Historical per-turn totals are not rehydrated into `TurnRecord` after a
  process restart; durable assistant messages remain reconstructible facts.
- `TurnEvent` terminal variants include `usage` with serde default for wire
  compatibility.
