# V-53: CQRS session read models

> Feature: [F-37](../features/F-37-materialized-read-models.md)
> Design: [D-53](../design/D-53-cqrs-session-read-models.md)
> Date: 2026-08-18

## Scope under test

| Acceptance criterion | Evidence |
|---|---|
| Live projection equals reopen from current read model and from journal rebuild | `generated_branch_history_converges_live_read_model_and_full_replay`; `append_reopen_and_idempotent_retry_converge` |
| Corrupt/missing current read model rebuilds from the journal | `corrupt_current_read_model_is_rebuilt_from_the_journal` |
| Foreign-generation current file is ignored | `foreign_generation_current_read_model_is_ignored` |
| Catalog inspect matches published revision | `rolls_segment_at_one_thousand_commits` asserts `inspect_facts` revision 1000; hostd `stores_and_recovers_agent_transcript_from_v4_journal` |
| Corrupt journal stays listable with `integrity_error` | `corrupt_v4_session_remains_listable_with_integrity_error` |
| Fork/import destination is independently correct | `fork_to_copies_agent_history_with_rewritten_session_id`; `branch_point_fork_keeps_ancestor_path_only`; `import_validates_then_atomically_publishes_without_merging_existing_destination` |
| Trajectory list/fetch after restart | `query_lists_and_fetches_runs_from_journal_events`; `turn_writes_durable_trajectory_records` |
| Open/resume without snapshot files | `persistent_turn_recovers_each_agent_private_transcript` asserts `readmodels/` and no `snapshots/` |
| Query LRUs removed | `LruMap`, `facts_cache`, `open_stores` LRU, trajectory decode LRU deleted |

## Results

| Test | Result |
|---|---|
| `piko-session-store` `--test journal` | 20 passed |
| `piko-hostd` `--lib` | 173 passed |
| `piko-hostd` `--test session_store` | 18 passed |
| `piko-hostd` `--test session_storage` | 6 passed |
| `piko-hostd` `--test trajectory_turn` | 1 passed |
| `cargo clippy -p piko-session-store -p piko-hostd --all-targets -- -D warnings` | clean |

## Notes

- Boundary snapshot write/read is gone. Rebuild is a full journal replay.
- Listing reads `readmodels/catalog.json` + `head.json` when aligned, and
  peeks only the open journal segment for F-31 integrity-error listing.
