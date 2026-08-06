# V-32: F-15 exact turn diffs

> Feature: [F-15](../features/F-15-observability.md)
> Design: [D-32](../design/D-32-exact-turn-diff.md)
> Date: 2026-08-06

## Evidence

| Criterion | Evidence |
|---|---|
| Exact edit/write before and after content | `edit_applies_unique_replacement`, `write_captures_exact_create_and_overwrite` |
| Private detail durable but model-hidden | `file_change_details_are_durable_but_not_model_visible` |
| Repeated mutations and net-zero rollup | `turn_file_changes_roll_up_to_net_diff` |
| Reconstruction avoids workspace reads | `durable_tool_change_rebuilds_turn_diff_without_workspace_read` |
| Live projection | observation tests and exhaustive client event handling |

## Commands

```bash
cargo test -p piko-orchd file_change
cargo test -p piko-hostd turn_file_changes_roll_up_to_net_diff
cargo test -p piko-hostd durable_tool_change_rebuilds_turn_diff_without_workspace_read
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
