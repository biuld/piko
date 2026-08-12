# V-41: F-30 per-agent usage acceptance evidence

> Feature: [F-30](../features/F-30-per-agent-usage.md)
> Design: [D-42](../design/D-42-per-agent-usage.md)
> Date: 2026-08-12

## Automated evidence

- `domain::sessions::tests::per_agent_usage_is_rebuilt_without_merging_instances`
  verifies that durable assistant usage is attributed by AgentInstance even
  when two instances share the same agent spec id.
- `application::sessions::helpers::usage_tests::execution_stats_count_runs_and_include_running_elapsed_time`
  verifies run counting plus completed and in-progress active-duration sums.
- `event::snapshots::usage_tests::missing_agent_usage_defaults_to_empty`
  verifies backward-compatible snapshot decoding.
- `app::tests::usage_tests::session_reconcile_projects_cumulative_usage`
  verifies that the TUI replaces its per-agent and session ledgers from the
  host snapshot.
- `app::tests::command_tests::usage_opens_modal_and_refreshes_host_snapshot`
  verifies `/usage` mounts the Usage surface and requests an explicit
  `StateSnapshot`.
- `app::tests::command_tests::local_slash_commands_exist_before_host_catalog_arrives`
  verifies `/usage` is present and `/status` is removed.
- Usage renderer unit tests cover duration formatting and AgentInstance label
  disambiguation; existing BottomBar formatter tests cover multi-currency and
  estimate-basis cost rendering reused by the panel.

## Commands

```text
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

All commands passed on 2026-08-12.

## Acceptance summary

- Hostd derives token/cost rows from schema-v4 accounting facts and derives
  run timing from journal execution events.
- Missing execution timing renders unavailable rather than as fabricated zero.
- Agent rows are stable, distinguish instances, preserve provider-native cost
  entries, and remain scrollable while session token/cost totals stay visible.
- No session duration is produced, so parallel AgentInstance time is not
  presented as additive wall-clock time.
