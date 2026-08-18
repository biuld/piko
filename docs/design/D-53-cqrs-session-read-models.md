# D-53: CQRS session read models

> Status: implemented
> Implements: [F-37](../features/F-37-materialized-read-models.md)

## Goal

Move every existing session query off journal replay. `piko-session-store`
publishes durable read models on the write path; hostd listing, trajectory
query, and open/resume read those models. Process-local LRUs, raw-event
caches, and F-31 boundary snapshots are removed.

## Constraints and non-goals

- F-31 journal format, checksum chain, segment rollover, and event
  vocabulary stay. Segments remain history, not a cache.
- `session.json` stays identity-only.
- Rebuild of a missing or stale read model is a full journal replay.
- No global catalog. No SQLite. No new query surfaces.
- Command-side memory of an attached writer stays. It is not an LRU of
  recently queried sessions.
- `island-rs` is unchanged.

## Proposed design

### Ownership

`piko-session-store` owns read-model files and the publish/rebuild
contract. hostd does not project journal events for list, trajectory, or
open. It reads `query_catalog` / `query_current` / `query_trajectory`.
Resolving a session id walks `session.json` identity files only. The
writer `Journal::open` path is not a query implementation.

Publication order after a successful journal append (F-31 Committed):

1. Apply the commit to the in-memory aggregate and trajectory projection
   (attached writer only).
2. Atomically replace `readmodels/catalog.json`,
   `readmodels/current.json`, and `readmodels/trajectory.json`.
3. Atomically replace `readmodels/head.json` last.

`head.json` is the Published watermark. A query also peeks the last
complete open-segment record (one object). If that tip matches `head`,
the model is served. A crash between the journal write and `head.json`
leaves tip ahead of `head`; the next query rebuilds. It does not parse
the rest of the open segment or any closed segment.

A failed read-model write does not retract the journal commit. The next
query or open rebuilds.

Idempotent append (same `commit_id`, matching payload) republishes read
models when they lag, then returns the existing commit.

### On-disk layout

```text
<session>/
  session.json
  writer.lock
  events/<start>-<end>.jsonl
  events/<start>-open.jsonl
  readmodels/head.json
  readmodels/catalog.json
  readmodels/current.json
  readmodels/trajectory.json
```

`snapshots/` is no longer created, written, or read. Existing snapshot
files are ignored.

Each read-model file is one JSON object, atomically replaced
(`tmp` + rename + dir sync), schema version `1`. Unknown or older
`schemaVersion` is treated as missing.

`head.json`:

```text
schemaVersion, sessionId, journalGeneration, revision, checksum
```

`catalog.json` adds `throughRevision`, `throughChecksum`, and the
existing list facts: `name`, `updatedAt`, `messageCount`,
`extraTreeCount`, `firstUserMessage`.

`current.json` is the `SessionAggregate` at that revision plus the same
envelope (`throughRevision`, `throughChecksum`, identity, generation).

`trajectory.json` is the joined run projection (same reducer hostd uses
today for F-36) plus the envelope.

### Ordinary query

**List.** For each session directory, read `session.json`, `head.json`,
and `catalog.json`. If they align, emit `SessionSummary` without opening
the journal or taking `writer.lock`. If they do not align, open once
(rebuild), emit the rebuilt catalog, and drop the handle. If identity is
readable but rebuild fails, emit `integrity_error` (F-31).

**Trajectory list/fetch.** Read `trajectory.json` when it matches
`head.json`. Otherwise open once, rebuild, serve. Paging, missing-record
counts, interrupted runs, and live follow stay in hostd; they consume
the published projection, not a decoded-event LRU.

**Open/resume.** Writer `open` takes the lock, repairs an incomplete
open-tail record, and reads the last complete open-tail commit. If
`current.json` matches that tip, the aggregate is loaded from the read
model and closed segments are not read. Otherwise `read_all` rebuilds
every read model. hostd `load_session_dir` keeps projecting from the
loaded aggregate; it no longer requires a prior replay in this process.

### Writer memory

An attached `SessionStore` holds the live aggregate and the live
trajectory projection so the next append does not re-read files. Dropping
the last handle releases both. The next command reloads from
`current.json` / `trajectory.json` (or rebuilds).

Same-process `Weak` identity of an open writer remains so two handles
share one lock and one reducer. That is not a capacity-bounded LRU.

### Removed caches

| Cache | Replacement |
|---|---|
| hostd `facts_cache` LRU (256) | `catalog.json` |
| hostd `open_stores` LRU (32) | no retained journal for query; `SessionStore::new` opens only when a command or rebuild needs the writer |
| hostd trajectory decode LRU (64) | `trajectory.json` |
| in-memory `raw_events` on the journal handle | trajectory read model |
| F-31 boundary snapshots and `inspect_facts` | `current.json` / `catalog.json` |
| `LruMap` | deleted with its last callers |

`list` / `summaries` must not call `cached_store` / `SessionStore::open`
on the aligned path.

### Rebuild

Rebuild replays every durable commit from the journal (existing
`read_all` + `apply_for_replay` + trajectory apply). It then writes the
three models and `head.json`. It does not read snapshot files.

### Create / fork / import

`create` publishes read models for the genesis commit before the
destination directory is visible. Fork/import publish destination read
models from the destination journal; they do not copy source
`readmodels/`. A leftover copied directory whose identity or generation
does not match is rebuilt on first query.

### Package impact

| Package | Change |
|---|---|
| `piko-session-store` | `readmodels/` module; publish on append/create; fast open from `current.json`; remove snapshot write/read and `raw_events` cache; `inspect_catalog` replaces `inspect_facts` |
| `piko-hostd` | list via `inspect_catalog`; trajectory via published projection; drop the three LRUs and `LruMap`; open still uses `load_session_dir` on a store that no longer replays when current is fresh |
| `piko-protocol` | none |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Incomplete final journal record: repaired on writer open, as today.
  List continues to serve the last published catalog.
- Unreadable or version-incompatible read model: discarded, rebuilt.
- Journal not replayable: session stays listable with `integrity_error`.
- Writer locked by another process: query still reads aligned files
  without the lock. Rebuild that needs the lock fails like today's open.

## Verification

- Store unit tests: append publishes aligned head+catalog+current+
  trajectory; reopen uses current without depending on snapshots;
  deleting any one read-model file rebuilds on next open; crash-between
  journal-and-head still lists the previous catalog; idempotent retry
  republishes lagging models; fork destination catalog matches
  destination identity.
- Differential: after a turn, catalog / trajectory / current-state
  fields equal a full replay of the same revision.
- hostd: `summaries` does not retain journal handles; trajectory query
  has no decode LRU; `LruMap` is gone.

## Alternatives considered

- **Keep snapshots for rebuild.** Rejected by F-37: current state is the
  always-current projection; a 1,000-commit copy is a second cache.
- **Append-only jsonl read models.** Rejected: projections change in
  place (run join, transcript ancestry). The journal is the append-only
  log.
- **hostd-owned files.** Rejected: append, create, and rebuild already
  live in the store; a second publisher would split Committed/Published.
- **Global catalog index.** Rejected by F-37: delete/fork/import stay
  directory-local.

## Key Decisions

1. **Store-owned publish, `head.json` last.** Makes Published a single
   file compare and keeps journal ACK independent of derived writes.
2. **Three files, atomic replace.** Matches the three query surfaces;
   format is an implementation detail of this design, not of F-37.
3. **List never takes the writer lock on the aligned path.** Required to
   drop the open-store LRU without serializing listing behind writers.
4. **Writer open peeks only the open tail** to detect unpublished
   commits, then either loads `current.json` or `read_all`.
5. **Drop snapshots and all query LRUs in the same feature.** F-37
   forbids leaving them as a fallback.

## Rollout

1. Store publish + `inspect_catalog` + list without `facts_cache` /
   snapshot listing.
2. Trajectory file + hostd query without decode LRU / `raw_events`.
3. `current.json` fast open, delete snapshot module usage, delete
   `open_stores` LRU and `LruMap`.

Landed as one change set; tests gate all three slices together.

## PR Plan

This lands in-tree as one implementation (not a stacked PR) matching the
three F-37 slices:

1. **session-store read models** — `readmodels/*`, append/create/open
   hooks, snapshot retirement, catalog/trajectory/current tests.
2. **hostd query paths** — list, trajectory, drop LRUs, load still
   consumes the store aggregate.
