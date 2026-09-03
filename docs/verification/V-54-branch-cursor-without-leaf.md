# V-54: Branch cursor without Leaf nodes

> Feature: [F-38](../features/F-38-branch-cursor-without-leaf.md)
> Design: [D-54](../design/D-54-branch-cursor-without-leaf.md)
> Date: 2026-08-18
> Updated: 2026-09-03

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Navigate to root user publishes empty cursor, no Leaf node | `in_memory_session_navigate_to_root_user_resets_current_leaf_without_leaf_node`; `persistent_session_navigate_to_root_user_clears_cursor_without_leaf_node` |
| Storage navigate moves cursor without a Leaf tree entry | `root_transcript_advances_persisted_leaf_across_reopen` |
| Label append does not move the cursor | same test after `append_entry(Label)` |
| Branch-point fork cursor is the fork entry | `branch_point_fork_keeps_ancestor_path_only` |
| Protocol has no Leaf type | `piko-protocol` compile; no `Leaf` / `LeafEntry` |
| Hidden non-content current node selects nearest ancestor | `load_selects_nearest_visible_ancestor_when_current_leaf_is_hidden` |
| Empty cursor projects an empty active branch | `empty_cursor_means_empty_active_branch`; `empty_cursor_has_no_model_context` |
| Backtrack resets an already-attached runtime before the next model request | E2E `backtrack_excludes_abandoned_messages_from_model_history` |
| Branch summary is model-visible context | `branch_summary_is_model_visible_context`; E2E `branch_summary_is_injected_into_the_continuation_context` |

## Results

| Test | Result |
|---|---|
| `piko-protocol` lib | 58 passed |
| `piko-hostd` `--lib` | 183 passed |
| `piko-hostd` `--test session_store` | 27 passed |
| `piko-hostd` `--test session_storage` | 9 passed |
| `piko-hostd` `--test server_jsonl` | 12 passed |
| `piko-tui` unit tests | 456 passed |
| `piko-tui --test terminal_e2e` | 2 passed |
| `piko-tui --test terminal_pty` | 2 passed |
| `piko-client-core` | all tests passed |
| `piko-session-store` | all tests passed (26 journal tests plus recovery/property suites) |
| `piko-e2e` | all suites passed |
| `piko-e2e --test session_read_models` | 4 passed |
| `piko-e2e --test session_branching` | 2 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |

## Notes

- `SessionTreeEntry` has no Leaf variant. Navigate writes only
  `BranchSelected`. Undeclared tree-entry payloads are not projected.
- Open/resume trusts `selected_tree_entry_id`. Branch-point fork writes
  that cursor to the fork entry.
- Navigation invalidates an attached orchd session; the next input rehydrates
  from the host-owned branch cursor.
