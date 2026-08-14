# F-31: Durable session journal

> Status: implemented (D-43/ADR-015/V-42)
> Priority: P0
> Source evidence: piko product decision; reliability review of F-09 schema-v3
> session persistence; F-28/F-29/F-30 accounting contracts
>
> This PRD supersedes the storage, replay,
> branching, and recovery behavior in [F-09](F-09-session-persistence.md).
> F-09 remains only the source for deferred session-list paging and startup
> prewarm until those behaviors are separately specified.

## Summary

piko preserves every committed session fact in one ordered, append-only
history owned by hostd. Live state, resumed state, agent transcripts, the
session tree, and usage accounting are deterministic projections of that same
history. A crash may discard explicitly transient streaming output, but it
must not make acknowledged content disappear, duplicate usage, move a branch,
or leave the client and model with different committed histories.

## Problem

Schema-v3 spreads authority across a mutable session manifest, per-agent
message shards, and process memory. A logical operation can update only some
of those locations before failing. Branch selection is separate from the
transcript head used by the runtime, recovery can append facts after the
in-memory projection was built, and a partially written final JSONL record can
make a complete session undiscoverable. Usage is reconstructed from message
and execution projections, so compaction, fork, or a projection bug can also
change historical accounting.

These are ownership problems rather than isolated serialization bugs. piko
needs one durable order and one replay rule for every acknowledged session
fact.

## User journeys

1. A user submits a message and sees streaming output. The stream remains
   visibly pending until its durable commit is acknowledged. Once committed,
   the same message appears after refresh and process restart.
2. A user navigates to an older entry and continues. The new message is a
   child of the selected branch point; the abandoned branch remains available
   and is not included in the new model context.
3. A root agent and several child agents run concurrently. Each private
   transcript resumes from its own committed parent while the session tree
   preserves a deterministic cross-agent presentation.
4. The host crashes while appending the final record. Reopening retains every
   previously acknowledged commit, safely discards only the incomplete tail,
   and reports that recovery occurred.
5. The host finds corruption before the final record. It refuses to invent a
   partial state, keeps the session discoverable, and reports an actionable
   integrity error.
6. A user opens usage reporting after resume, compaction, navigation, or fork.
   Token and provider-native cost totals match the unique model work actually
   performed and are not double-counted by inherited history.
7. A newer piko writes an optional extension understood by no older reader.
   The older reader preserves or safely ignores it as declared. If the event
   changes required session semantics, the older reader refuses to open with
   an explicit upgrade-required error.

## In scope

- One authoritative append-only sequence of durable facts per session.
- Acknowledgement only after a complete durable commit.
- Deterministic reconstruction of session state from durable history.
- Explicit distinction between transient realtime output and committed
  transcript content.
- Message/tree parentage independent from physical append order.
- Durable branch selection and continuation from the selected branch point.
- Per-AgentInstance private transcript reconstruction.
- Atomic logical commits for facts that must remain consistent together.
- Durable, idempotent usage and provider-native cost facts attributed to model
  step, execution, turn, AgentInstance, and session.
- Append-only usage corrections rather than historical mutation.
- Fork lineage that does not count inherited usage as newly incurred usage.
- Optional rebuildable snapshots and indexes for bounded startup latency.
- Safe incomplete-tail recovery and fail-loud middle-corruption handling.
- Event and envelope versioning, namespaced extensions, and declared handling
  for unknown event kinds.
- Forward evolution within the new journal generation without rewriting
  historical records.

## Out of scope

- Persisting every token or byte of realtime model deltas by default.
- Treating transient progress, approval panels, or editor state as committed
  session facts.
- Provider invoice reconciliation, currency conversion, budgets, or quotas.
- Sharing one writable session between multiple host processes.
- Reading, importing, migrating, or continuing schema-v3 sessions after the
  schema-v4 cutover.
- Cursor-paged session listing and model-connection startup prewarm retained as
  residual F-09 work.
- A general-purpose event-sourcing framework for unrelated piko data.

## Behavior and states

### Commit visibility

A session change moves through these states:

1. **Proposed**: a command or runtime result has produced facts but hostd has
   not durably accepted them.
2. **Pending presentation**: realtime content may be shown with a distinct
   non-durable identity. It is never returned as committed history.
3. **Committed**: the complete logical commit is durable, has a stable session
   revision and event identities, and may update live projections.
4. **Published**: clients have received projections of the committed facts.

Publication may be retried or reconstructed. It cannot precede the committed
state. A durable success followed by a process crash before publication is
replayed on reopen.

### Async host boundary

Hostd command and turn handlers never execute session filesystem work directly
on Tokio runtime workers. Their repository and session-store ports are async;
the filesystem adapter runs each complete synchronous journal operation on a
bounded blocking executor and awaits its result. The permit covers the whole
operation, including lock acquisition, reducer preflight, append, sync,
rollover, replay, import, or fork as applicable.

Cancellation of the awaiting host task does not cancel a blocking operation
after it has started. The durable operation finishes according to the journal
contract, and idempotent retry or replay observes its result. Queueing is
bounded so concurrent recovery/import work cannot grow the blocking pool
without limit.

### Replay and snapshots

Replaying all recognized required events in revision order produces the same
durable state as applying those events live. A snapshot records the revision
through which it was built. Invalid, incompatible, or missing snapshots are
discarded and rebuilt from durable history; they never replace that history as
authority.

Every 1,000 durable commits, piko closes the current journal segment at that
revision, opens the next segment, and schedules a snapshot through the same
revision. Segment boundaries and snapshot boundaries therefore align. Snapshot
work does not delay or weaken commit acknowledgement; a failed snapshot is
retried and replay from the closed segments remains the fallback.

If the host stops after synchronizing the boundary commit but before publishing
the closed segment name, reopen recognizes the exact full open segment as a
verified rollover-in-progress state, closes it, opens the next segment, and
reports recovery. More than 1,000 records in an open segment remains
corruption.

The following equality is normative:

```text
live committed projection
  == full-history replay
  == valid snapshot plus history-tail replay
```

### Session tree and agent transcripts

Physical append order answers when a fact committed. Parent identifiers answer
where it belongs in a tree or transcript. A committed message identifies its
AgentInstance, private-transcript parent, and session-tree parent where those
parents exist.

Branch selection is a durable cursor, not a rewrite of message history. A new
root turn declares the selected base revision and branch point. Stale work that
targets a superseded revision fails before it can silently extend another
branch.

The active model context is the selected message ancestry plus applicable
compaction and session-context facts. Siblings and abandoned descendants are
not included merely because they committed earlier.

### Accounting

Every billable or observable model step has at most one effective usage fact,
identified independently from a transcript message. It retains the normalized
token components and provider-native cost ledger defined by F-28/F-29, together
with stable attribution and the pricing/policy provenance required to explain
the estimate later.

Retries with the same usage identity are idempotent. A later provider final
usage value appends a correction linked to the original fact. Aggregation
applies unique base facts and corrections; it does not scan the visible branch
or infer cost from the current transcript.

Forked history may display inherited usage separately, but inherited facts do
not become newly incurred global or destination-session spend. New model calls
that resend inherited context record their own real input usage normally.

### Recovery and corruption

- An incomplete final record with no durable acknowledgement is truncated to
  the last verified boundary.
- A complete and verified final record is retained even if no client observed
  its publication.
- A session directory is published only after its `session_created` genesis
  commit is durable. A published directory without genesis is invalid rather
  than an empty session.
- Corruption before the recoverable tail prevents normal replay. The session
  remains listable with an integrity-error state and its files are not silently
  deleted or skipped.
- Recovery appends any required interruption facts as one logical commit, then
  applies that same commit to the live projection before publishing a
  reconciled snapshot.
- A session writer rejects stale expected revisions, duplicate identities with
  conflicting payloads, and invalid tree/transcript parents.

### Compatibility

This compatibility contract begins with schema-v4 and governs its future event
evolution; it does not provide schema-v3 compatibility.

The durable envelope and each event kind have independent versions. Additive
optional fields have documented defaults. Extension fields are namespaced and
cannot redefine core semantics.

An unknown event is either:

- explicitly ignorable and preserved/skipped without changing required state;
  or
- required, in which case an older reader returns an upgrade-required error.

Known historical event versions are converted in memory by deterministic
read-time adapters. Durable historical bytes are never rewritten as part of
ordinary open or resume.

## Acceptance criteria

- [x] An acknowledged message survives a crash at every point after durable
      acknowledgement and appears exactly once after replay.
- [x] A crash at every byte boundary of an unacknowledged final append retains
      all prior commits and safely removes only the incomplete tail.
- [x] A crash after synchronizing commit 1,000 but before segment rollover
      retains that commit and completes rollover during reopen.
- [x] Session creation publishes no final directory before the durable genesis
      commit, and open rejects a published journal with no genesis.
- [x] Middle corruption leaves the session discoverable and produces an
      explicit integrity error rather than `not found`.
- [x] Applying a generated event sequence live, replaying it from the
      beginning, and replaying it from each valid snapshot boundary produce
      equivalent durable projections.
- [x] Navigating to an ancestor and continuing creates a sibling branch whose
      model context excludes abandoned descendants.
- [x] Root and child AgentInstances resume only their own private transcript
      ancestry while the host session tree remains deterministic.
- [x] Interrupted execution recovery atomically records its terminal state,
      report, and model-visible abort marker and includes them in the first
      reconciled snapshot.
- [x] Usage is counted exactly once across retry, replay, observation recovery,
      compaction, navigation, and snapshot rebuild.
- [x] Usage corrections change aggregates without mutating the original fact.
- [x] Forking preserves lineage and history without adding inherited usage to
      incurred destination or global totals.
- [x] An older compatible reader ignores a declared optional event/extension;
      an unknown required event fails with an upgrade-required diagnostic.
- [x] Removing every snapshot and index still permits complete session replay.
- [x] No client-visible committed entry lacks a durable event identity and
      revision.
- [x] Async host command and turn paths execute no session filesystem operation
      directly on Tokio runtime workers, and blocking storage concurrency is
      bounded.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Durable authority | One ordered append-only history owned by hostd | Eliminates split-brain manifest/shard/memory state |
| Realtime durability | Deltas are pending by default; final facts are durable | Avoids write amplification while making visibility honest |
| Branch representation | Explicit parent graph plus durable selected cursor | Append order cannot represent branch ancestry |
| Usage authority | Independent immutable usage facts and corrections | Accounting must survive transcript projection changes |
| Fork accounting | Inherited history is not newly incurred usage | Prevents double-counting across sessions |
| Snapshot authority | Rebuildable cache only | Recovery correctness cannot depend on a second truth |
| Unknown required events | Fail with upgrade required | Silently ignoring state-changing facts corrupts replay |
| Schema-v3 handling | Unsupported after cutover; remove the v3 reader, writer, DTOs, and compatibility branches | A clean replacement is safer than permanent dual-generation behavior |

## Fusion decisions (codex-rs)

This feature is driven by piko's reliability and accounting requirements. The
codex-rs thread store remains modeling evidence only.

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Durable thread rollout | kept (adapted) | One piko-native host-owned durable history |
| Resume from committed history | kept (adapted) | Deterministic reducer and snapshot-tail replay |
| Thread fork/branch | kept (adapted) | Explicit message/tree parents and branch cursor |
| Interrupted rollout markers | kept (adapted) | Atomic recovery commit including abort marker and terminal facts |
| codex thread-store schema/coupling | rejected | piko owns its journal schema and preserves the hostd/orchd split |

## Open questions

1. Should inherited usage be shown in the first accounting UI revision or
   retained only as lineage metadata until a reporting journey requires it?

## Reference evidence

- [F-09 session persistence](F-09-session-persistence.md)
- [F-28 provider-native cost accounting](F-28-provider-native-cost-accounting.md)
- [F-29 provider-pluggable billing](F-29-provider-pluggable-billing.md)
- [F-30 per-agent usage](F-30-per-agent-usage.md)
- [D-26 session branch-point fork](../design/D-26-session-branch-point-fork.md)
- piko schema-v3 reliability review, 2026-08-12
