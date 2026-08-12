# D-43: Event-sourced session store

> Status: implemented (V-42)
> Implements: [F-31](../features/F-31-durable-session-journal.md)
> Decisions: [ADR-015](../decisions/ADR-015-host-owned-session-journal.md),
> [ADR-013](../decisions/ADR-013-provider-native-cost-ledger.md),
> [ADR-014](../decisions/ADR-014-registered-billing-policies.md)

## Goal

Introduce a schema-v4 session store with one host-owned canonical event
journal, deterministic live/replay projections, explicit tree and private
transcript parentage, durable accounting facts, safe tail recovery, and an
evolution contract for future event versions. The implementation moves these
responsibilities into a dedicated `piko-session-store` crate without moving
host application authority into orchd or the storage layer.

## Constraints and non-goals

- Rust 2024 and existing workspace dependency rules apply.
- `piko-protocol` remains a shared DTO leaf and contains no filesystem logic.
- Hostd remains authoritative for durable user-visible state and client
  publication.
- Orchd owns AgentInstance execution and submits commit intents; it never
  writes session files.
- JSONL remains the human-inspectable append format.
- Schema-v4 replaces schema-v3 outright at cutover. The final tree contains no
  v3 reader, writer, DTO, import, migration, or dual-write path.
- Realtime deltas remain transient by default. The durable journal represents
  committed state, not every streamed token.
- Snapshots and indexes are disposable projections.
- This design does not implement session-list paging, connection prewarm,
  invoice reconciliation, budgets, or currency conversion.
- This design does not create a general event-sourcing framework for settings,
  auth, or unrelated packages.

## Implemented design

### Package boundary

Add `packages/session-store` with crate name `piko-session-store`:

```text
packages/session-store/src/
  lib.rs
  accounting.rs
  aggregate.rs
  aggregate_queries.rs
  error.rs
  journal.rs
  journal_io.rs
  projection.rs
  replay.rs
  schema.rs
  segments.rs
  snapshot.rs
tests/
  journal.rs
```

The initial crate is session-specific. Journal mechanics stay behind an
internal module rather than being prematurely published as a generic
`piko-event-journal` crate.

Dependency direction:

```text
piko-protocol
      ^
      |
piko-session-store
      ^
      |
piko-hostd --------> piko-orchd-api
      |
      +------------> piko-orchd
```

`piko-session-store` must not depend on hostd application types,
`ServerMessage`, TUI state, orchd actors, provider clients, or settings
resolution. The host adapter supplies a resolved session directory and maps
runtime commit DTOs to store commands/events.

### Schema-v4 layout

```text
<session-dir>/
  session.json
  events/
    00000000000000000001-open.jsonl
  snapshots/
    00000000000000001000.json
  writer.lock
```

`session.json` contains only storage-generation identity:

```json
{
  "schemaVersion": 4,
  "sessionId": "session-1",
  "cwd": "/workspace/project",
  "createdAt": 1786512345000,
  "journalGeneration": "journal-1"
}
```

It does not contain messages, agents, inboxes, executions, branch cursors,
usage, or mutable display metadata. Those are reduced from events.

The store rolls the journal at fixed 1,000-commit boundaries. After revision
1,000, for example, it closes `00000000000000000001-open.jsonl` as
`00000000000000000001-00000000000000001000.jsonl`, opens
`00000000000000001001-open.jsonl`, and schedules snapshot revision 1,000.
Segment and snapshot names use zero-padded revisions so lexical and numeric
order match.

### Commit record

One JSONL line is one atomic logical commit. A commit may contain several
domain events that share a revision and must become visible together:

```json
{
  "schemaVersion": 4,
  "sessionId": "session-1",
  "revision": 42,
  "commitId": "commit_01...",
  "committedAt": 1786512345678,
  "producer": {"component": "hostd", "version": "0.1.0"},
  "causationId": "execution-1",
  "correlationId": "turn-1",
  "events": [
    {
      "eventId": "event_01...",
      "type": "message_committed",
      "version": 1,
      "compatibility": {
        "requiredReaderVersion": 4,
        "ignorable": false
      },
      "payload": {},
      "extensions": {}
    }
  ],
  "extensions": {},
  "checksum": {"algorithm": "crc32", "value": "..."}
}
```

`revision` is session-global, starts at one, and increases by exactly one per
commit line. Events inside a commit are applied in array order and addressed by
stable `eventId`; a cursor may use `(revision, event_index)` when it needs to
identify a position inside a batch.

The checksum covers the record without its checksum field using one stable
canonical JSON encoding owned by the crate. Integrity verification must not
depend on `HashMap` iteration order. CRC32 detects torn or accidental
corruption; it is not an authenticity signature.

Every successfully appended record ends with `\n`. A record without the final
newline is never acknowledged and is an incomplete-tail candidate. The writer
serializes, validates, and checksums the entire line before opening the file
for append.

### Journal API and commit order

The public surface remains narrow:

```rust
pub struct SessionStore;

impl SessionStore {
    pub fn create(path: &Path, identity: NewSession) -> Result<OpenedSession>;
    pub fn open(path: &Path, options: OpenOptions) -> Result<OpenedSession>;
    pub fn append(
        &self,
        expected_revision: u64,
        commit: ProposedCommit,
    ) -> Result<DurableCommit>;
    pub fn verify(&self) -> Result<VerificationReport>;
    pub fn write_snapshot(&self) -> Result<SnapshotRef>;
}
```

`append` executes under the session writer lock:

1. Confirm the current verified revision.
2. Reject a mismatched `expected_revision`.
3. Validate event IDs, idempotency keys, parents, and aggregate invariants.
4. Apply proposed events to a cloned aggregate as reducer preflight.
5. Encode one complete commit line and checksum.
6. Append the line and newline and call `sync_data` for durable acknowledgement.
7. Return the exact durable commit.

Hostd applies only the returned commit to its live aggregate and then publishes
derived client events. If hostd fails after step 6, reopen replays the durable
commit. It must not manufacture another equivalent commit to compensate for a
publication failure.

### Idempotency and concurrency

`commitId`, `eventId`, and domain identities such as `messageId` and `usageId`
are stable retry keys.

- Repeating a `commitId` with identical canonical content returns the original
  durable acknowledgement.
- Reusing an identity with different content returns an idempotency conflict.
- A stale expected revision returns a revision conflict; the caller re-reads
  and re-decides rather than attaching to a new tail.
- Runtime commits include the message/tree base selected when work was
  admitted. The writer never substitutes the physical tail.

The writer lock is process-wide and filesystem-backed. The current in-process
per-path mutex is insufficient for two host processes. Lock acquisition is
fail-closed; concurrent writable hostd processes remain unsupported.

### Event catalog

The first schema generation defines these required event families:

| Event | Durable fact |
|---|---|
| `session_created` | Session identity, cwd, and root identity |
| `session_metadata_changed` | Name and other durable user metadata |
| `agent_created` | AgentInstance identity, parent, and recoverable spec |
| `agent_lifecycle_changed` | Open/closed/terminated/unavailable lifecycle |
| `agent_selected` | Host-visible selected AgentInstance cursor |
| `agent_input_queued` | Durable queued input and admission identity |
| `agent_input_dequeued` | Queue cancellation or transition into a run |
| `execution_started` | Execution attribution, prompt digest, and start time |
| `execution_finished` | Terminal outcome, report, and finish time |
| `message_committed` | Message, private/tree parents, and execution attribution |
| `branch_selected` | Current tree cursor and root model-context base |
| `inbox_report_committed` | Detached result delivered to an AgentInstance |
| `inbox_report_consumed` | Durable report consumption |
| `compaction_recorded` | Summary and covered/retained ancestry |
| `world_state_advanced` | Root world-state diff baseline |
| `model_continuity_changed` | Last provider/model used by the session |
| `todo_list_replaced` | Per-AgentInstance todo list or clear |
| `usage_recorded` | Unique attributed token/cost fact |
| `usage_corrected` | Append-only correction linked to a usage fact |
| `session_forked` | Source lineage and imported history boundary |
| `tree_entry_recorded` | Unknown-preserving host-visible tree entry payload and stable ancestry fields |
| `optional_annotation` | Optional metadata with no required reducer effect |

Payloads use stable IDs and integer timestamps. Persisted open-ended
identifiers use strings or unknown-preserving wrappers rather than closed Rust
enums that reject future values.

### Tree and private transcript model

`message_committed` separates three orders:

```rust
pub struct MessageCommittedV1 {
    pub message_id: MessageId,
    pub agent_instance_id: AgentInstanceId,
    pub agent_parent_message_id: Option<MessageId>,
    pub tree_parent_entry_id: Option<EntryId>,
    pub execution_id: Option<ExecutionId>,
    pub source_turn_id: Option<TurnId>,
    pub message: Message,
}
```

- Journal revision is commit chronology.
- `agent_parent_message_id` is the private model transcript ancestry for that
  AgentInstance.
- `tree_parent_entry_id` is the host-visible session tree ancestry.

Neither parent is inferred from the final JSONL line. The reducer validates
that referenced parents exist and belong to the expected scope. Agent-local
sequence values, if later exposed for paging, are monotonic cursors and are not
required to remain dense after branch projection or fork.

`branch_selected` contains the selected tree entry and resolved root-agent base
message. Navigation appends this event and changes the branch cursor. The next
root run is admitted with that base. The model-context reducer walks parent
ancestry, applies the latest relevant compaction, and excludes siblings.

A child AgentInstance resumes from its own `agent_parent_message_id` chain.
Cross-agent display placement uses `tree_parent_entry_id` and never leaks a
child's private messages into the root model transcript.

### Deterministic aggregate

`piko-session-store` exposes the durable aggregate used by both live apply and
replay:

```rust
pub struct SessionAggregate {
    pub revision: u64,
    pub identity: SessionIdentity,
    pub tree: SessionTree,
    pub agents: AgentProjection,
    pub executions: ExecutionProjection,
    pub accounting: AccountingProjection,
}

impl SessionAggregate {
    pub fn apply(&mut self, commit: &DurableCommit) -> Result<(), ApplyError>;
}
```

The reducer is pure with respect to filesystem, clock, model registry, and
client state. Events contain all durable facts needed for replay. Hostd may map
the aggregate into application views, but it must not maintain a second
mutable copy of durable parentage, lifecycle, or accounting rules.

### Atomic recovery commits

Recovery does not append facts after building state without applying them.
Open proceeds as follows:

1. Verify the journal and repair only an incomplete final record.
2. Load a valid snapshot and replay its tail, or replay from revision zero.
3. Decide recovery events for each non-terminal execution.
4. Append one batch per independent recovery group containing the abort
   marker, terminal execution outcome, report, queue/inbox consequences, and
   any required cursor update.
5. Apply returned durable commits through the same reducer.
6. Return the first reconciled host snapshot.

The first snapshot after open therefore contains every recovery fact already
present on disk.

### Accounting facts

Accounting is independent from transcript visibility:

```rust
pub struct UsageRecordedV1 {
    pub usage_id: UsageId,
    pub attribution: UsageAttribution,
    pub provider: String,
    pub model_id: String,
    pub api_surface: Option<String>,
    pub pricing_policy_id: Option<String>,
    pub pricing_revision: Option<String>,
    pub usage: piko_protocol::Usage,
    pub incurred: bool,
}

pub struct UsageAttribution {
    pub session_id: SessionId,
    pub agent_instance_id: AgentInstanceId,
    pub turn_id: Option<TurnId>,
    pub execution_id: ExecutionId,
    pub model_step_id: String,
}
```

The existing F-28/F-29 `Usage` and provider-native cost ledger remain the
normalized quantities. The event adds identity, attribution, and
pricing-policy provenance. Amounts retain their existing fixed/decimal
representation and native currency/basis; the store introduces neither float
money nor implicit currency summation.

A completed model-step commit normally batches its final message,
`usage_recorded`, and applicable execution progress. Partial provider-reported
usage for a failed or cancelled step is committed when it is a real provider
fact.

`usage_corrected` names the original `usageId`, has its own correction identity,
and carries a replacement value in v1. Replacement semantics avoid ambiguous
component arithmetic. Duplicate corrections are idempotent.

`AccountingProjection` maintains rebuildable buckets by session,
AgentInstance, turn, execution, provider/model, currency, and estimate basis.
Global aggregation includes only incurred facts. Fork import records source
lineage and may expose inherited usage, but it does not copy source usage as
newly incurred destination events.

The crate exposes read-only accounting queries over a reduced session:

```rust
pub struct UsageQuery {
    pub agent_instance_id: Option<AgentInstanceId>,
    pub turn_id: Option<TurnId>,
    pub execution_id: Option<ExecutionId>,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub incurred_only: bool,
}

pub trait AccountingView {
    fn summarize(&self, query: &UsageQuery) -> UsageSummary;
    fn facts(&self, query: &UsageQuery) -> impl Iterator<Item = EffectiveUsageFact>;
}
```

Cross-session, day, or workspace reports merge per-session facts or summaries
using source `(sessionId, usageId, correctionId)` identities. A future global
accounting index may cache that merge, but it records its covered session
revisions and is fully rebuildable. It must never become a second billing
ledger or accept usage writes independently from session journals.

### Compatibility and extensions

Compatibility begins at schema-v4. An independent reader-capability version
allows event evolution within schema-v4 without changing the storage
generation. The registry/upcaster contract supports future schema-v4 event
evolution and does not decode schema-v3 storage.

The reader first decodes a raw envelope without a closed event enum:

```rust
pub struct RawEvent {
    pub event_id: String,
    pub event_type: String,
    pub version: u32,
    pub compatibility: Compatibility,
    pub payload: serde_json::Value,
    pub extensions: BTreeMap<String, serde_json::Value>,
}
```

The event catalog resolves `(event_type, version)` to a decoder. Schema-v4
starts with version 1 for its known events; adding a later known version also
adds its pure in-memory conversion into the current aggregate input.

- Known old versions are converted in memory through pure functions once such
  versions exist.
- Upcasting never rewrites the journal.
- An unknown event with `ignorable = false` returns `UpgradeRequired` before
  producing a partial aggregate.
- An unknown event with `ignorable = true` is retained in raw replay/export
  views and skipped by required projections.
- `requiredReaderVersion` improves diagnostics but does not replace the
  ignorable rule.

Commit and event envelopes both have `extensions` maps. Keys must be
namespaced, for example `piko.dev/provider-metadata`. Extensions may add
annotations but cannot alter parentage, accounting, lifecycle, idempotency, or
other required semantics. Such a semantic change requires a new event version
or type.

The raw event remains available alongside its typed/upcast representation for
verification, export, and compatibility-preserving operations. Known readers
never rewrite a source event merely to fill defaults. Optional information that
must survive processing by older readers belongs in a namespaced extension;
an old reader is not expected to synthesize unknown future core fields when it
creates a semantically new event in another session.

Snapshots have an independent schema version. An unsupported snapshot is
discarded and does not prevent journal replay when the journal is supported.

### Snapshots

A snapshot contains the session ID, journal generation, through revision,
aggregate schema version, aggregate/accounting projection, and source event
digest through the revision.

Snapshot creation writes a unique temporary file, synchronizes it, renames it
atomically, and synchronizes the snapshots directory. There is no authoritative
snapshot pointer: open scans snapshot filenames newest-first and validates the
candidate against the journal prefix.

Snapshot creation clones an immutable aggregate at a revision, serializes
outside the writer critical section, and publishes only while that revision
remains a valid journal prefix. Removing all snapshots must leave a
fully recoverable session.

At every revision divisible by 1,000, the writer durably commits that revision,
syncs and closes the current segment, atomically publishes its closed name, and
opens/syncs the next segment before accepting revision 1,001. It then schedules
snapshot creation from the immutable aggregate at the same boundary revision.

Segment rollover is part of the journal write path: failure to publish or open
the next segment prevents later commits, and reopen resumes from the last
verified segment boundary. Snapshot serialization, sync, and publication remain
asynchronous. Snapshot failure keeps the previous snapshot, reports a
diagnostic, and retries without blocking later commits or changing journal
correctness.

### Filesystem durability and discovery

The adapter applies these rules:

- Use unique temporary names for identity and snapshot creation.
- Synchronize content before rename and the containing directory after a
  durability-sensitive create or rename.
- A final byte sequence without newline can be truncated to the previous
  verified newline.
- A newline-terminated invalid final record is corruption unless a proven
  fault-injection rule establishes it was never acknowledged.
- Invalid JSON, checksum, or revision in the middle is never skipped.
- Session enumeration returns a corrupt summary row instead of silently
  omitting the directory.
- Repair returns a report with original length, truncated bytes, last verified
  revision, and reason. Hostd may surface it without injecting filesystem
  details into model input.

### Hostd integration

Hostd wraps the store in a session application service:

```text
command/runtime intent
  -> load current aggregate/revision
  -> decide proposed domain events
  -> append(expected revision, batch)
  -> apply returned durable commit
  -> update host application view
  -> publish protocol projections
```

Hostd does not optimistically mutate durable state and roll it back after a
write failure. TUI realtime deltas continue through the observation path with
a pending identity. `message_committed` finalizes/replaces the pending item;
cancellation removes unresolved pending content and publishes durable terminal
facts.

Session open receives `OpenedSession.aggregate`. Hostd maps it into its view
and attaches orchd using per-agent transcript projections from the aggregate.
It no longer re-reads agent files or independently infers message parents.

### Orchd integration

Execution and agent commit ports remain the durability boundary visible to
orchd. Their DTOs gain:

- expected session revision or admission token;
- explicit message private/tree parents;
- stable model-step and usage identity;
- causation/correlation identities;
- one logical batch for runtime facts that must commit atomically.

Orchd treats success as durable. A revision conflict is an admission/recovery
error, not permission to attach to the latest tail. Retry retains the same
commit and idempotency identities.

### Fork

Fork never filters and copies source JSONL lines. Hostd asks the source
aggregate for retained tree ancestry and referenced AgentInstances, then
creates a v4 destination with a genesis/import batch containing:

- new session identity and timestamps;
- source session/revision lineage;
- retained messages with explicit parents and normalized journal order;
- retained metadata and AgentInstances under the fork policy;
- empty live queues and non-terminal executions;
- a cleared world-state diff baseline where F-09 requires it;
- no incurred destination copies of source usage facts.

Message IDs may remain stable for imported parent references. Commit/event IDs
and revisions are new; origin event IDs remain lineage metadata.

### Schema-v3 removal

The production cutover is a hard replacement. Once schema-v4 reaches the
acceptance gate:

- remove the schema-v3 session store and repository modules from hostd;
- remove schema-v3 persisted DTOs, ports used only by that adapter, and its
  fixtures/tests;
- remove generation detection, legacy open/list/import, and compatibility
  branches;
- create, list, open, resume, navigate, fork, and delete only schema-v4
  sessions;
- leave existing schema-v3 directories untouched on disk, but do not discover
  or interpret them.

There is no automatic or explicit v3-to-v4 migration/import feature in this
proposal. Deleting unsupported on-disk v3 data is also not part of cutover.

## Package impact

| Package | Change |
|---|---|
| `piko-session-store` | New crate: journal, schema/events, reducers, accounting, snapshots, recovery, compatibility handling, and crash tests |
| `piko-protocol` | Add stable commit identities and compatible durable/client projection DTO fields; no filesystem types |
| `piko-hostd` | Decide events, apply durable commits, publish views, and remove the schema-v3 persistence implementation at cutover |
| `piko-orchd-api` | Add explicit parents, admission/revision identity, usage identity, and atomic-batch commit contract |
| `piko-orchd` | Produce stable identities/ancestry and keep deltas transient until durable acknowledgement |
| `piko-llmd` | Preserve normalized usage and billing-policy provenance required by usage events |
| `piko-tui` | Distinguish pending and committed rows and surface recovery/integrity state |

## Reusable infrastructure

No `island-rs` change required. The journal is piko session infrastructure and
has no second consumer that justifies a shared generic library.

## Failure and cancellation

| Failure | Result |
|---|---|
| Serialization or reducer preflight failure | No append and no live mutation |
| Stale expected revision | Conflict; no reparenting to the current tail |
| Crash before newline/sync | No acknowledgement; incomplete tail repaired on open |
| Crash after sync before apply/publish | Replay applies the commit exactly once |
| Duplicate retry with identical payload | Return original acknowledgement |
| Duplicate identity with changed payload | Idempotency conflict |
| Unknown optional event | Preserve/skip as declared |
| Unknown required event | Upgrade-required error; no partial projection |
| Middle corruption | Integrity error; session remains discoverable |
| Snapshot corruption/incompatibility | Ignore snapshot and replay journal |
| Recovery interrupted | Durable recovery commits replay on the next open |
| Cancelled model stream | Pending output is finalized/removed; real committed usage remains |
| Concurrent host writer | Filesystem lock conflict; second writer fails closed |

No cleanup path deletes an unreadable session automatically. Destructive
repair beyond truncating a provably incomplete tail requires a future explicit
user or administrator workflow.

## Verification

### Pure and property tests

- Event sequences produce identical live, full-replay, and snapshot-tail
  aggregates.
- Message DAG ancestry, sibling branches, selection, and active model context.
- Root and child private transcripts never cross-contaminate.
- Accounting idempotency, corrections, attribution, cost-ledger grouping, and
  fork incurred/inherited separation.
- Unknown optional/required events and every supported event version.
- Invalid parents, transitions, revisions, and duplicate identities.

Generate valid command sequences and continuously assert:

```text
incremental aggregate
== full replay aggregate
== snapshot plus tail aggregate
```

Generated tree/fork cases must retain valid ancestry, exclude abandoned
siblings from active context, and preserve accounting totals.

### Journal and crash tests

- Fault injection before and after append write, newline, sync, live apply,
  and publication boundaries.
- Truncate at every byte of the final record and verify the last acknowledged
  revision.
- Corrupt every middle position and verify fail-loud discovery.
- Independent process/file-handle writer lock and stale revision tests.
- Snapshot temporary-file, rename, directory-sync, corrupt-candidate, and segment
  boundary tests.

### Integration and compatibility tests

- hostd intent -> durable acknowledgement -> HostState/TUI projection.
- First crash-reopen snapshot includes interruption marker and report.
- Pending realtime content is finalized only by a durable event.
- Concurrent multi-agent commits get one global order and independent private
  parents.
- Session listing distinguishes healthy, recovered, and corrupt sessions.
- The final dependency/module graph contains no schema-v3 persistence adapter
  or compatibility branch.
- Checked-in golden JSONL fixtures cover each event/snapshot version.
- Unknown optional fixture succeeds; unknown required fixture returns the
  stable upgrade-required diagnostic.

## Alternatives considered

- **Patch schema-v3 ordering:** rejected because multiple authoritative files
  still require transactions and accounting remains projection-dependent.
- **Canonical per-agent event streams:** rejected because cross-agent tree,
  recovery, metadata, and accounting need a global commit order. Per-agent
  indexes may remain derived.
- **SQLite first:** deferred. Transactions do not remove the need for stable
  domain events/reducers and conflict with the requested inspectable JSONL.
- **Persist every realtime delta:** rejected as default because of write
  amplification and because uncommitted output is not a durable fact.
- **Keep storage inside hostd:** rejected because schema, replay, accounting,
  and fault-injection mechanics form a cohesive package boundary.
- **Generic persistence crate:** rejected until another bounded context needs
  the mechanics.
- **Dual-write v3/v4:** rejected because mismatches recreate split authority.

## Rollout

1. Add `piko-session-store` with envelope fixtures, writer lock,
   append/verify/tail recovery, and crash-point harness. No production session
   uses it yet.
2. Add event catalog, aggregate reducers, compatibility registry, and
   replay-equivalence property tests.
3. Add usage/correction facts and accounting projections preserving F-28/F-29
   ledgers.
4. Add disposable snapshots and verify equivalence at snapshot boundaries.
5. Integrate every session command with schema-v4 and drive live host/client
   projection only from returned durable commits while the branch is under
   development.
6. Extend runtime commit ports with explicit ancestry, admission/revision,
   atomic batch, and usage identities; remove v4 parent inference.
7. Switch navigation, active-context reconstruction, and normalized fork to
   the aggregate tree model.
8. Switch interrupted execution recovery and first-open reconcile to atomic
   recovery commits; expose integrity/recovery diagnostics.
9. Run workspace, golden compatibility, randomized crash/replay, and long
   session performance verification.
10. Perform the hard cutover: remove schema-v3 storage code, DTOs, fixtures,
    tests, generation detection, and import/open compatibility; verify that
    only schema-v4 session paths remain reachable.
