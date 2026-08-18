# V-54: Branch cursor without Leaf nodes

> Feature: [F-38](../features/F-38-branch-cursor-without-leaf.md)
> Design: [D-54](../design/D-54-branch-cursor-without-leaf.md)
> Date: 2026-08-18

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Navigate to root user publishes empty cursor, no Leaf node | `in_memory_session_navigate_to_root_user_resets_current_leaf_without_leaf_node`; `persistent_session_navigate_to_root_user_clears_cursor_without_leaf_node` |
| Storage navigate moves cursor without a Leaf tree entry | `root_transcript_advances_persisted_leaf_across_reopen` |
| Label append does not move the cursor | same test after `append_entry(Label)` |
| Branch-point fork cursor is the fork entry | `branch_point_fork_keeps_ancestor_path_only` |
| Protocol has no Leaf type | `piko-protocol` compile; no `Leaf` / `LeafEntry` |
| Hidden non-content current node selects nearest ancestor | `load_selects_nearest_visible_ancestor_when_current_leaf_is_hidden` |

## Results

| Test | Result |
|---|---|
| `piko-protocol` lib | 58 passed |
| `piko-hostd` `--lib` | 173 passed |
| `piko-hostd` `--test session_store` | 18 passed |
| `piko-hostd` `--test session_storage` | 6 passed |
| `piko-hostd` `--test server_jsonl` | 12 passed |
| `piko-tui` | 375 passed |
| `piko-client-core` tests | 76 passed (contract/helpers/m4/operation/operational/projection/scale/session) |
| `cargo clippy -p piko-protocol -p piko-hostd -p piko-tui --all-targets -- -D warnings` | clean |

## Notes

- `SessionTreeEntry` has no Leaf variant. Navigate writes only
  `BranchSelected`. Undeclared tree-entry payloads are not projected.
- Open/resume trusts `selected_tree_entry_id`. Branch-point fork writes
  that cursor to the fork entry.
