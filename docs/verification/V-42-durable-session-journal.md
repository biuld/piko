# V-42: F-31 durable session journal acceptance evidence

> Feature: [F-31](../features/F-31-durable-session-journal.md)
> Design: [D-43](../design/D-43-event-sourced-session-store.md)
> Decision: [ADR-015](../decisions/ADR-015-host-owned-session-journal.md)
> Async boundary: [ADR-017](../decisions/ADR-017-bounded-blocking-session-storage.md)
> Date: 2026-08-15

## Automated evidence

- `piko-session-store/tests/journal.rs` verifies append/reopen convergence,
  full-proposal idempotency, stale/conflicting retries, and single-writer
  handle reuse. A child-process test verifies the filesystem writer lock.
- `every_torn_commit_byte_boundary_recovers_the_verified_prefix` truncates a
  final commit at every byte and verifies that replay keeps exactly the last
  checksum-verified prefix.
- `incomplete_tail_is_repaired_but_middle_corruption_fails` verifies the
  recoverable-tail/fail-loud-middle distinction. Host integration coverage
  verifies that a corrupt schema-v4 session remains listable with an integrity
  diagnostic.
- `checksum_verifies_original_float_spelling_without_json_round_trip` verifies
  that checksum validation uses the persisted JSON bytes, accepts a writer
  spelling that would shorten after floating-point decoding, and still rejects
  a real byte change.
- `rolls_segment_at_one_thousand_commits` verifies that revision 1,000 seals
  the `1-1000` segment, opens `1001-open`, and publishes only the revision-1000
  snapshot.
- `piko-session-store/tests/crash_recovery.rs` verifies that a synchronized
  revision-1,000 commit left in the open segment is retained and normalized on
  reopen, that a published session without its genesis commit is rejected, and
  that parallel session creation safely shares the staging container.
- `snapshot_plus_tail_matches_full_replay_and_corrupt_snapshot_is_disposable`
  verifies snapshot+tail/full replay equivalence and that journal replay does
  not depend on the snapshot cache. Closed-boundary snapshots are rebuilt when
  missing or corrupt, including when the journal already has a later tail.
- `generated_branch_history_converges_live_snapshot_tail_and_full_replay`
  builds a deterministic generated branch DAG through a segment/snapshot
  boundary and compares the complete aggregate across all three paths.
- `snapshot_from_another_journal_generation_is_rejected` verifies that session
  identity, journal generation, boundary revision, and checksum chain bind a
  snapshot to one exact journal prefix.
- Branch, private-transcript, fork, interrupted-execution, durable queue, and
  inbox integration scenarios run in `piko-hostd/tests/session_store.rs` and
  `piko-hostd/tests/session_storage/`.
- Accounting coverage verifies immutable usage facts, filtered aggregation,
  idempotent correction, and stable totals across retry, navigation,
  compaction, snapshot, and replay. Fork integration verifies inherited
  transcript usage is not incurred by the destination. Unknown optional
  events are skipped and unknown required events return upgrade-required.
- Extension coverage verifies namespaced commit/event metadata round-trips,
  missing optional maps default safely, and unqualified keys are rejected.
- Host open integration verifies the first reconciled snapshot already
  contains the atomic interrupted-execution report, terminal state, and
  model-visible abort marker.
- Host repository integration verifies that schema-v4 import validates a
  synchronized staging copy, publishes it atomically, and refuses to merge a
  second import into the existing destination.
- Host storage-adapter coverage verifies that one shared semaphore bounds
  complete blocking operations and that cancelling an async waiter does not
  cancel an already-started synchronous filesystem transaction. All session
  repository/query ports are async and execute whole operations outside Tokio
  runtime workers; low-level journal code remains synchronous.

## Commands

```text
cargo fmt --all
PIKO_DEV_SOURCE_ROOT=$PWD cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

All commands passed on 2026-08-15.

## Invariants

- The JSONL commit journal is the sole durable authority; the live aggregate,
  full replay, and valid snapshot-plus-tail replay use the same reducer.
- A segment contains exactly 1,000 commits when sealed. Its matching snapshot
  is through that same boundary revision and is disposable/rebuildable.
- Every commit carries session identity, journal generation, global revision,
  stable commit/event identities, an extension map, and a checksum chained to
  the previous commit.
- Schema-v3 readers, writers, shard files, migration, import, and dual-write
  paths are absent from the production storage graph.
- Async host callers never execute session journal filesystem work directly;
  the shared blocking boundary limits cross-session concurrency to eight.
