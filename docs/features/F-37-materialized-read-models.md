# F-37: CQRS read models for session queries

> Status: implemented (F-37/D-53/V-53)
> Priority: P1
> Source evidence: piko product decision; cold-query cost on production
> session `ae9c8fdd` (2026-08-17); F-31 journal authority and F-36 trajectory
> query surfaces

## Summary

Session **commands** still append to the F-31 journal. Session **queries**
never replay that journal on the ordinary path. Each existing query surface
is served from a durable, write-time-updated read model that already reflects
the latest published revision. The journal remains the single source of
truth; every read model is a disposable projection and can be rebuilt from
the journal. Once a read model is the query contract, the caches that used
to hide replay — process-local fact/event caches, bounded LRU retainers
of recently replayed sessions, and F-31 boundary snapshots — are
removed, not kept as a fallback.

## Problem

F-31 made the session an event-sourced history. That is the right write
model: one append-only order, deterministic replay, crash-safe recovery.
It is the wrong read model. Every current query is implemented as "open the
history and project it again":

- **Session listing** reconstructs catalog fields from each session's
  history (or from a snapshot plus the open tail). A first cold list of five
  sessions measured **3.5s**; the large session alone was ~2.8s. After
  snapshot/tail work it is still ~2.1s. Repeats are fast only while a
  process-local cache lives.
- **Trajectory list/fetch** (F-36) decodes the session's raw history to join
  runs. First view of 3,321 events measured ~1.1s+.
- **Open/resume** rebuilds the committed session projection and then derives
  transcripts, tree, agents, and ledgers from it. History load alone was
  ~2.4s on 1,677 commits.

Those costs are not a missing index on one screen. They are the event-sourcing
tax: **the read path is another write path**. Snapshot-plus-tail, in-memory
event caches, per-process fact caches, and bounded LRUs of recently opened
journals / decoded trajectories only hide replay inside one lifetime (and
only for the sessions that still fit). They do not change the contract. A
restart, a cache eviction, a new query surface, or a session that has never
been opened in this process pays the full tax again.

piko needs CQRS for session state: the journal answers "what was committed,
in order"; read models answer "what is the current catalog / trajectory /
working session". Replay is a recovery and verification tool, not a query
implementation.

## User journeys

1. The developer restarts piko and opens the session list. Every session
   shows name, first message, counts, and timestamps immediately. History
   is not scanned.
2. The developer opens Session History on a large session that this process
   has never loaded. Work summaries appear immediately; opening one item
   does not decode the session journal.
3. The developer resumes a large session. The transcript, tree, agents, and
   usage already match the last published commit. Resume does not rebuild
   that state from history.
4. hostd is killed after a commit is durable but before its read models
   catch up. The next query detects the lag, rebuilds from the journal, and
   then returns the same result a clean write would have published.
5. The developer deletes a session. Its read models disappear with it.
   Listing and Session History retain no dangling reference.
6. The developer forks or imports a session. The destination appears in the
   list with its own correct catalog and can be opened without serving the
   source session's projections.

## In scope

- A CQRS split for every **existing** session query surface:
  - **Session catalog** — list fields already shown today (identity, name,
    first user message, message and tree-entry counts, created/updated
    times, path, integrity error).
  - **Trajectory** — F-36 list (paged, newest first) and fetch (full run,
    paged), including missing-record and interrupted-run semantics.
  - **Session current state** — the committed projection required to open,
    resume, and answer in-session committed queries: selected branch, session
    tree, per-agent private-transcript ancestry, agent instances, inbox and
    queued inputs, executions needed for resume, and F-32 incurred ledgers.
- Read models are durable across process restart and are updated from each
  newly appended commit **before that commit is published** to query
  consumers (F-31 Committed → Published).
- Each read model records the session revision it is current through, and a
  published **revision watermark** lets a query detect lag without reading
  journal history.
- Missing, unreadable, version-incompatible, or stale read models are
  discarded and rebuilt from the journal, then served. Rebuild output equals
  a full replay of the same history.
- Read models live with the session they describe. There is no global
  cross-session index. Listing visits per-session catalog entries.
- New query surfaces added after this feature must publish a read model.
  They must not introduce another replay-on-read path.
- F-52 applies this rule to Session History through a dedicated ordered history
  projection joined revision-consistently with current and trajectory models.
- Retirement of the caches this feature replaces: process-local fact and
  raw-event caches, every bounded LRU that retains replayed journals or
  decoded query projections for cheaper subsequent reads, and F-31
  boundary snapshots. This PRD supersedes F-31's snapshot-plus-tail path
  as the way to bound query and open latency.

Delivery may land in slices (catalog, then trajectory, then current state)
without changing the contract above. A cache is removed when the read
model that replaces it is published, not in a later cleanup PRD.

## Out of scope

- Changing the journal format, checksum chain, segment rollover, or event
  vocabulary (F-31). Segments are history, not a cache.
- Replacing the journal with another storage engine.
- Dropping the command-side working copy of a session that is currently
  open for append (reducer, expected-revision and parent checks). That is
  write memory for attached writers only, not an LRU of recently queried
  sessions. It is discarded when the session is not being written, and the
  next command reloads **current state** from the read model.
- Cursor-paged session listing and startup prewarm (residual F-09).
- A global catalog across working directories.
- Asynchronous or eventually-consistent read-model updates. Consumers must
  not observe a published commit whose read models have not caught up.
- New product screens, fields, or analytics beyond making the existing
  queries correct and journal-free on the ordinary path.
- Using replay as the implementation of **commands** that must read history
  (verify, some fork/import construction). Those are not query surfaces.

## Behavior and states

### Command versus query

- **Command** (create, append, navigate, compact, fork, import, delete):
  writes the journal under the F-31 commit rules. The journal is
  append-only. History is never rewritten to satisfy a query.
- **Query** (list, trajectory list/fetch, open/resume, in-session committed
  reads such as transcript, tree, and usage): reads the matching read
  model. The ordinary path does not read journal history.

A commit moves through the F-31 visibility states. This feature adds one
normative rule to **Published**:

```text
Committed  = journal has accepted the complete logical commit
Published  = every in-scope read model reflects that revision
             and query consumers may be notified
```

Publication cannot precede Committed. A durable commit that crashes before
publication is recovered by rebuild on the next query, the same way F-31
replays an unpublished success on reopen.

Read-model update failure must not retract a durable journal commit. The
commit stays Committed and unpublished; the next query rebuilds.

### What each read model answers

**Session catalog.** One entry per session, sufficient to render the list
without opening the session. Fields match today's list contract, including
`integrity_error` when the session is discoverable but not replayable.
Genesis publication includes an empty catalog entry (no first message, zero
counts). Tree entries that are not messages still contribute to the existing
combined sequence count.

**Trajectory.** The F-36 run graph as already specified: list summaries
and full run records, observational and best-effort, with child-run links,
interrupted runs (no terminal), and explicit missing records. Live following
observes the same records a later fetch returns, as those records become
Published.

**Session current state.** The committed working session, not a flat dump of
every historical message. Per-agent transcripts are the private ancestry
from each agent head (F-31). Navigate, compaction, and branch selection
change this projection in place. Open/resume and in-session committed
queries are the same read.

### Incremental update

Each newly appended commit is applied to every in-scope read model. A commit
that does not change a model's visible content still advances that model's
revision so it stays aligned with the watermark.

Read models are projections, not a second event log. They may be replaced
wholesale when a commit changes them. Append-only belongs to the journal.

### Ordinary query

1. Read the revision watermark (no history scan).
2. Confirm the watermark against the last complete open-segment record
   only (one JSON object, not the journal history).
3. Read the read model. If it is present, readable, compatible, and at the
   watermark, serve it.
4. Otherwise discard it, rebuild from the journal, publish the rebuilt
   model, and serve that.

List, open/resume, and trajectory list/fetch are this query path. They
must not open the writer, parse the open segment, or load every other
session in order to answer one query. Session identity files are enough
to resolve a path by id.

A query that hits step 4 is a **rebuild path**, not the ordinary path. It
replays the journal from the beginning. That cost is accepted because it is
recovery; it must be rare after a clean write. Rebuild does not consult
snapshots or process-local history caches.

### Caches that go away

These exist only to make replay cheaper. After the matching read model
lands they have no remaining job:

- Per-process fact caches and raw-event caches that make a second list,
  trajectory view, or open cheap in the same lifetime.
- Bounded in-process LRUs of recently replayed sessions: list-summary
  facts, retained journal handles used so list/load can skip a second
  replay, and decoded trajectory projections. A later query of the same
  session re-reads the durable read model; it does not hit a recently-used
  replay set. There is no capacity of "hot" sessions whose history stays
  in memory for query.
- Snapshot-plus-tail listing and open.
- Boundary snapshots of the committed projection. The current-state read
  model is the always-current form of that snapshot; a second, lagging copy
  on every 1,000-commit boundary is not kept.

F-31's equality `live == full replay == snapshot + tail` is replaced, for
query and open, by the published-read-model equality below. Snapshot files
are no longer written or read.

### Empty, error, and restoration

- **Empty session:** listed and openable from the genesis catalog and
  current-state projections.
- **Stale or missing read model:** rebuild, then serve. The user-visible
  result is correct, not an error.
- **Unreadable or version-incompatible read model:** same as missing.
- **Journal not replayable:** the session remains listable with
  `integrity_error` (F-31). Catalog must not disappear, and must not present
  a stale healthy entry as if the journal were intact.
- **Delete:** session directory removal is the only catalog removal.
- **Fork/import:** the destination is published with its own read models
  already correct. Source projections are never served as the destination.
  Copied leftover projections that do not match destination identity or
  watermark are discarded, not shown.

### Lifecycle and authority

- Read models are stored with the session they describe. Delete, fork, and
  import therefore cannot leave cross-session references.
- Read models have no independent checksum authority. The journal checksum
  chain is unchanged.
- Each read model carries a schema version. An unknown or older version is
  treated as missing and rebuilt.
- The following equality is normative and extends F-31's live/replay
  equality:

```text
published query result
  == live committed projection after the same revision
  == rebuild-from-journal query result
```

## Acceptance criteria

Shared (every slice):

- [x] After a commit is published, list, trajectory, and current-state
      queries return the same observable result as a full journal replay of
      that revision (differential validation against F-31 / F-32 / F-36).
- [x] A missing, unreadable, version-incompatible, or stale read model is
      rebuilt on the next query; the journal remains the source of truth.
- [ ] A crash after Committed and before Published is healed by rebuild;
      subsequent published queries match replay.
- [x] Read-model update failure does not drop or rewrite an acknowledged
      journal commit.
- [x] Deleting a session leaves no catalog or trajectory reference.
- [x] Fork/import publishes a destination whose catalog and current state
      match the destination journal, with no source-session fields leaking
      into the first list or open.

Slice 1 — session catalog:

- [x] After process restart, listing sessions does not read journal history.
      For on the order of 100 sessions it completes without the multi-second
      cold scans measured on `ae9c8fdd`.
- [x] A session whose journal cannot be replayed remains listed with
      `integrity_error` and is not shown as a healthy up-to-date entry.

Slice 2 — trajectory:

- [x] After process restart, list-runs and fetch-run do not decode journal
      history on the ordinary path, and preserve F-36 paging, missing-record,
      interrupted-run, and live-follow semantics.

Slice 3 — session current state:

- [x] After process restart, open/resume and in-session committed reads
      (transcript ancestry, tree, agents, usage ledgers) do not rebuild from
      journal history on the ordinary path.

Cache retirement (with the slice that replaces each cache):

- [x] Slice 1 removes the list-summary fact LRU, snapshot-plus-tail
      listing, and any LRU of journal handles retained so listing can
      reuse a prior replay. Listing does not read snapshot files.
- [x] Slice 2 removes the decoded-trajectory LRU and the raw-event cache
      as a trajectory query path.
- [x] Slice 3 removes boundary snapshot write/read, snapshot-plus-tail
      open, and any LRU of journal handles retained so open/resume can
      reuse a prior replay. Rebuild of current state is a full journal
      replay.
- [x] After all three slices, no bounded in-process retainer of replayed
      session history remains for query. Only sessions currently attached
      as writers keep command-side memory.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| What is this feature? | CQRS for existing session queries | Replay is the write model's recovery tool, not a read implementation |
| What remains authoritative? | The F-31 journal | Read models are disposable; crash recovery and verification stay journal-first |
| Which queries move off replay? | Catalog, trajectory, and session current state | Those are the queries that today replay; leaving any of them on replay keeps the tax |
| What is "session current state"? | The committed working session (branch, tree, private-transcript ancestry, agents, resume queues, F-32 ledgers) | Open/resume is a query over current state, not a flat message dump and not a second aggregate authority |
| When do read models update? | From each new commit, before Published | Consumers must not see a commit they cannot query |
| Does a failed read-model update fail the journal commit? | No | Derived data must not weaken journal acknowledgement |
| How is lag detected without replay? | Published revision watermark versus each model's through-revision | History scan cannot be the freshness check |
| Global list index? | No | Per-session catalog entries keep delete/fork/import local |
| Are read models a second event log? | No | The journal is append-only; projections may be replaced |
| Checksums on read models? | No | Structural failure, version mismatch, or watermark lag triggers rebuild |
| Schema evolution? | Version per read model; mismatch rebuilds | Silent field defaults would publish a wrong query result |
| Process-local fact/event caches? | Removed when the replacing read model lands | They only hid replay inside one process |
| Bounded LRU of recently replayed sessions (list facts, journal handles, decoded trajectories)? | Removed with the replacing slice | A query re-reads the durable read model; it must not depend on still being in a hot set |
| Boundary snapshots? | Removed with session current state | Current state is the always-current projection; a lagging 1,000-commit copy is a second cache |
| Rebuild acceleration after snapshots go? | Full journal replay | Recovery may be slow; the ordinary path must not need a cache |
| Live reducer of an attached writer? | Kept | Command-side memory, not a query cache; reload from current-state read model |
| New query surfaces later? | Must ship a read model | Forbids another replay-on-read feature |

## Open questions

None that block this PRD.

## Reference evidence

- Production session `ae9c8fdd` (1,677 commits / 3,321 events), 2026-08-17:
  history load ~2.4s, snapshot load ~75ms, tail replay ~16ms, session open
  ~2.8s; first list of five sessions ~3.5s.
- [F-31 durable session journal](F-31-durable-session-journal.md) — append-only
  authority, Committed/Published, integrity-error listing. This feature
  supersedes F-31 boundary snapshots as the latency contract.
- [F-32 session bookkeeping](F-32-session-bookkeeping.md) — incurred ledgers
  are journal-derived; this feature materializes them for query, it does not
  add a second usage store.
- [F-36 agent run trajectory](F-36-agent-run-trajectory.md) — list/fetch,
  paging, best-effort missing records, interrupted runs, live follow.
- [F-09 session persistence](F-09-session-persistence.md) — residual
  cursor-paged listing remains out of scope.
