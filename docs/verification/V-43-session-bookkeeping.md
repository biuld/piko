# V-43: Session bookkeeping

> Feature: [F-32](../features/F-32-session-bookkeeping.md)
> Design: [D-44](../design/D-44-session-bookkeeping.md)
> Date: 2026-08-13

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Occupancy matches orchd `message_tokens` | `domain::bookkeeping::occupancy::tests` |
| Compaction no longer uses `ceil(chars / 4)` | occupancy tests + `window_fraction_guard_scales_retrigger_to_resolved_model` rearm `8032` |
| Incurred ledger still accumulates once | `step_usage_accounts_into_turn_and_session`, `per_agent_usage_is_rebuilt_without_merging_instances` |
| Chrome `used` stays provider fill | `HostState::usage_updated_event` via bookkeeping projection |

## Reproduction

```bash
cargo test -p piko-orchd --lib domain::transcript::tokens
cargo test -p piko-hostd --lib domain::bookkeeping
cargo test -p piko-hostd --lib application::compaction
cargo test -p piko-hostd --test state_resume step_usage
```

## Result

2026-08-13, macOS, `cargo test` as above plus
`cargo clippy -p piko-hostd -p piko-orchd --all-targets -- -D warnings`.

| Suite | Result |
|---|---|
| `piko-orchd` `domain::transcript::tokens` | 3 passed |
| `piko-hostd` `domain::bookkeeping` | 5 passed |
| `piko-hostd` `application::compaction` | 3 passed |
| `piko-hostd` `domain::compaction` | 6 passed |
| `piko-hostd` `domain::sessions` | 8 passed |
| `piko-hostd --test state_resume step_usage` | 2 passed |
| `piko-hostd --test compaction_reconcile` | 6 passed |
| clippy `-D warnings` on hostd + orchd | clean |

## Invariants

- Message-entry occupancy equals `piko_orchd::transcript::message_tokens`.
- Compaction trigger and cut-point consume `estimate_entry_tokens`.
- Journal usage facts remain the only durable incurred store.
