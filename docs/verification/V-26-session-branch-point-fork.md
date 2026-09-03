# V-26: Session branch-point fork

> Feature: [F-09](../features/F-09-session-persistence.md)
> Design: [D-26](../design/D-26-session-branch-point-fork.md)
> Date: 2026-08-04
> Updated: 2026-09-03

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Full-session clone remains independent full copy (minus baseline / transient queues) | `full_clone_clears_world_state_baseline_and_transient_queues` in `packages/hostd/tests/session_store_cases/branch_point_fork.rs` |
| Branch-point fork retains only the ancestor path through `entry_id` | `branch_point_fork_keeps_ancestor_path_only` |
| Source session unchanged after fork | `branch_point_fork_keeps_ancestor_path_only` (reload source; `m1..m3` intact) |
| Forked `current_leaf_id` equals branch-point entry | `branch_point_fork_keeps_ancestor_path_only` |
| World-state baseline cleared on both fork modes | full-clone test + branch-point assert `world_state_baseline.is_none()` |
| Unknown `entry_id` fails closed | `branch_point_fork_rejects_unknown_entry` |
| Active-turn guard on command path | `apply_session_fork` returns `ActiveTurnExists` before storage write (mirrored navigate guard; covered by inspection + navigate regression) |
| Fork lineage records the requested branch point | `branch_point_fork_keeps_ancestor_path_only` inspects `SessionForkedV1.source_tree_entry_id` |
| Agents outside the retained path are excluded | cross-process E2E `spawn_agent_round_trips_from_jsonl_hostd_through_orchd_and_back` forks before the child and observes only the root AgentInstance |

## Commands

```bash
cargo test -p piko-hostd --test session_store branch_point
cargo test -p piko-hostd --test session_store full_clone
```

## Results

| Test | Result |
|---|---|
| `branch_point_fork_keeps_ancestor_path_only` | pass |
| `branch_point_fork_rejects_unknown_entry` | pass |
| `full_clone_clears_world_state_baseline_and_transient_queues` | pass |

## Notes

- The current implementation writes schema v4 journals; fork keeps the current
  schema and does not migrate or rewrite the source journal.
- Incomplete-execution interruption on open remains V-01 / recovery coverage
  (not re-asserted here).
- Sibling-branch truncation is covered by the same `active_branch_entries`
  walk as compaction; linear path test is the executed fixture.
