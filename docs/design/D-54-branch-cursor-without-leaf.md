# D-54: Branch cursor without Leaf nodes

> Status: accepted
> Implements: [F-38](../features/F-38-branch-cursor-without-leaf.md)
> Decisions: none new (F-31 branch cursor; F-37 current state)

## Goal

Stop writing a tree node as the navigate/backtrack record.
`SessionNavigate` and in-memory navigate persist only `EventData::BranchSelected`.
`SessionTreeEntry` has no Leaf variant. The published cursor is
`SessionAggregate.selected_tree_entry_id` (snapshot `current_leaf_id`).

## Constraints and non-goals

- Journal event vocabulary is unchanged. Do not add a new event kind or a
  `reason` field in this slice.
- Do not rewrite historical bytes. Retired `tree_entry_recorded` payloads
  whose `entry_type` is not a current graph kind are not projected.
- Do not replay the journal on tree/timeline/open to find the cursor.
- No previous-cursor stack.
- File-size ceiling still applies; navigate stays in the existing session
  command module.

## Proposed design

### Write path

```text
SessionNavigate
    → resolve target (user/custom → parent; else self; root user → None)
    → optional BranchSummary as TreeEntryRecorded (+ BranchSelected to it)
    → BranchSelected { selected_tree_entry_id, root_base_message_id }
    → current-state read model
```

`JsonlSessionRepository::navigate(dir, target_id)` commits only
`BranchSelected`. `parent_id` / `agent_id` and any returned tree node go away.

`root_base_message_id` is the target when that id is a committed message
on the root agent; otherwise `None` (including the empty-cursor reset).

Idempotent: if `selected_tree_entry_id` already equals the target, write
nothing.

### What still advances the cursor

| Write | Cursor |
|---|---|
| Root-agent `MessageCommitted` / tool call | `BranchSelected` to the new message (already) |
| Compaction, branch summary, custom message | `BranchSelected` to the new entry |
| Label, session info, model/thinking/tools notice, custom metadata | no `BranchSelected` |
| Navigate | `BranchSelected` only |

In-memory `HostState::append_entry` matches that table. Navigate without
storage calls `select_branch` and does not insert a cursor node.

### Read path

`project_session` materializes only `SessionTreeEntry` kinds that
`recognizes_recorded_type` accepts. Retired payloads stay in the journal
and in the aggregate map but never become graph nodes. Active path is the
parent walk from `current_leaf_id` / `selected_tree_entry_id`.

Open/resume trusts that cursor.

`active_branch_entries` without an explicit cursor returns an empty branch. A
specialized caller that intentionally needs a best-effort tip must choose that
tip explicitly before calling the lineage walk. Clients and hostd apply the
same rule. After navigation, hostd invalidates any attached orchd session so the
next input reattaches the root AgentInstance from the new durable cursor rather
than retaining the abandoned in-memory transcript.

When the cursor points to non-message content, `root_base_message_id` is the
nearest root-AgentInstance message on that entry's ancestry. Branch summaries
and custom messages are projected into model-visible `Context` messages with
stable source provenance.

### Protocol

`Leaf` / `LeafEntry` / `leaf_target_id()` are deleted.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Remove Leaf from `SessionTreeEntry` |
| `piko-hostd` | Navigate = `BranchSelected`; project only current entry kinds |
| `piko-tui` | No Leaf match arms or fixtures |
| `piko-session-store` | `BranchSelected` reducer unchanged; root transcript query treats an absent cursor as empty |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

Unknown target still fails navigate before write. A turn in progress still
returns `ActiveTurnExists`. Branch-summary cancellation still leaves the
cursor unchanged. A committed `BranchSelected` that crashes before
read-model publication rebuilds on the next query (F-37).

## Verification

- hostd: navigate to root user publishes empty cursor (memory + storage).
- hostd: storage `navigate` after two messages sets `current_leaf_id`
  without adding a tree entry.
- E2E: continuing after backtrack excludes abandoned user messages from the
  next gateway request, including when orchd was already attached.
- E2E: a generated branch summary reaches the next gateway request as Context.
- hostd: label/session-info append does not move `selected_tree_entry_id`.
- tui: hidden non-content current node still selects nearest visible
  ancestor.
- Compaction lineage tests still walk parent ids from the supplied cursor.

## Alternatives considered

- **Keep writing a cursor node and hide it in the default filter.**
  Rejected: the graph and the cursor stay coupled; CQRS already has the
  cursor.
- **New `branch_switched` event with from/to.** Rejected: `BranchSelected`
  is the cursor fact.
- **Keep Leaf on the wire so old journals decode.** Rejected: no
  compatibility path for a retired node kind. Undeclared payloads are
  simply not projected.

## Rollout

1. Docs (F-38 / this design).
2. Storage and in-memory navigate write only `BranchSelected`.
3. Remove the Leaf wire type; project only current entry kinds.
4. Tests and V-54.
