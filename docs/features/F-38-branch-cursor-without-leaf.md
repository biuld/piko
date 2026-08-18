# F-38: Branch cursor without Leaf nodes

> Status: implemented (F-38/D-54/V-54)
> Priority: P1
> Source evidence: piko product decision after F-31/F-37; Session Tree
> still persists navigation as a pi-style Leaf tree node

## Summary

The selected conversation position is a durable **cursor**, not a node in
the session tree. Navigating, backtracking, and continuing a branch change
that cursor. The tree itself stays a parent/child graph of conversation
content. Ordinary tree and timeline reads take the cursor and the graph
from the current-state read model (F-37). They do not invent an active
branch by scanning history or by walking Leaf pointer nodes.

## Problem

F-31 already records branch selection as a durable fact and F-37 already
exposes `selected` position on current state. The Session Tree and
in-session navigate path still follow the older pi model: each switch
appends a **Leaf** entry into the same tree used for messages, tools, and
compaction. Default filters hide those nodes, but they still occupy the
parent graph, inflate entry counts, and force every reader to know that
"some nodes are actually the cursor."

That mixes three different things:

1. the conversation graph (what was said, and under which parent);
2. the current position (which path the next turn continues);
3. a navigation breadcrumb that is neither content nor a query result.

After CQRS, (2) belongs on the current-state projection. (3) is leftover
from a file format that had no separate cursor fact. Keeping Leaf as a
tree node fights the journal and the read model.

## User journeys

1. The user opens Session Tree, selects an assistant message on another
   branch, and confirms. Timeline rebuilds onto that ancestry. No new tree
   row appears. After restart, the same branch is still active.

2. The user selects an earlier user message. The editor fills with that
   text and the next submit becomes a sibling of the original. Abandoned
   descendants stay on the tree. Resume still opens on the backtracked
   position, not the abandoned tip.

3. The user navigates to the first user message. The session position is
   before any message (empty cursor). Submitting starts a new root branch.
   Restart preserves the empty cursor.

4. Compaction or a new root message happens after a switch. The next turn
   continues from the new content on the selected branch.

## In scope

- Treat branch selection as a durable cursor fact already in the journal
  vocabulary. Navigate writes only that fact (plus an optional branch
  summary, which is content).
- Stop creating Leaf nodes on any write path, including in-memory
  sessions and storage-backed navigate.
- Derive the active branch by walking parents from the current cursor
  through the conversation graph.
- Serve tree, timeline, and resume from the current-state cursor and
  graph (F-37). Do not replay the journal to discover the active branch.
- Remove the Leaf wire type. Tree-entry payloads whose recorded type is
  not a current graph kind are not projected. There is no Leaf decode
  path and no Leaf-specific filter.
- Only conversation-graph content advances the cursor when appended
  (messages, tool calls, compaction, branch summaries, custom messages).
  Bookkeeping rows (labels, session info, model/thinking/tool-set
  notices) do not move the cursor.
- Backtrack stays a Session Tree gesture, not a new fact kind: user and
  custom messages target their parent and refill the editor; other rows
  target themselves; the root user message clears the cursor.

## Out of scope

- Changing parentage rules for messages or private transcripts (F-31).
- A stack of previous cursors or an "undo last switch" command.
- Rewriting historical journal bytes.
- Redesigning Session Tree filters, folding, or visual connectors.
- Fork/clone session files (F-09 / D-26). Those still copy graph + cursor.

## Behavior and states

### Conversation graph versus cursor

The graph records content and its parent. The cursor names one graph
node, or names none (before the first message). Siblings and abandoned
descendants remain in the graph. They are not in the active model
context unless the cursor walks through them.

### How the active branch is obtained

```text
active branch = parent walk from current cursor to the roots
current cursor = latest published branch-selection fact
              = current-state selected tree entry
```

Absence of a cursor means there is no selected node. Readers must not
fall back to "last appended row" or "last Leaf target" to invent one,
except a purely local compaction helper that has been given no cursor
and must pick a tip from the graph it was handed.

### Navigate

While the session is idle:

1. Resolve the actual target from the selected row (backtrack rules
   above).
2. Optionally record a branch-summary content node at the fork point.
   The cursor then moves to that summary.
3. Persist a branch-selection fact whose selected id is the target
   (or empty for the root reset).
4. Publish current state. Clients rebuild Timeline and Session Tree
   from that state.

Selecting the already-selected node is a no-op.

### Continue after navigate

The next committed root-agent message parents to the current cursor and
advances the cursor to itself. Abandoned branches stay reachable in
Session Tree.

### Restoration

Open/resume reads the current-state cursor. A crash after a committed
selection and before publication recovers by rebuild (F-37). The
recovered cursor equals the last committed branch-selection fact.

## Acceptance criteria

- [x] Navigate to a non-user entry moves the published cursor to that
      entry and adds no Leaf (or other cursor) node to the tree.
- [x] Navigate to a user or custom message moves the cursor to its
      parent, returns the message text for the editor, and adds no Leaf
      node.
- [x] Navigate to the root user message publishes an empty cursor and
      returns that prompt text.
- [x] After navigate, restart/reopen shows the same cursor and the same
      active-branch timeline.
- [x] A new message after backtrack is a sibling of the original and
      does not include abandoned descendants in model context.
- [x] The protocol has no Leaf entry type. Undeclared tree-entry
      payloads are not projected; the cursor is only the branch-selection
      fact.
- [x] Appending a label or session-info fact does not move the cursor.
- [x] Ordinary tree/timeline/open paths do not scan the journal to
      compute the active branch.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Is the cursor a tree node? | No. Cursor is a selection fact. | Graph and position are different; F-31 already separated them |
| Persist switch? | Yes, as the existing branch-selection fact | Resume and next-turn parentage require a durable cursor |
| Persist backtrack as its own fact? | No. Same selection fact, different target | Backtrack is how the UI chooses the target |
| Keep Leaf on the wire? | No | Cursor is the selection fact; no decode path for a retired node kind |
| Undo last switch? | Not now | Resume needs only the latest cursor; journal already audits selections |

## Fusion decisions (codex-rs)

Not derived from codex-rs. The rejected model is piko's earlier pi-style
Leaf pointer node, not a codex thread-store type.

| behavior | Decision | piko landing / rationale |
|---|---|---|
| pi Leaf as in-file cursor node | rejected | Cursor is F-31 branch selection + F-37 current state |
| pi backtrack (user row → parent + editor) | kept (adapted) | Same gesture; persist as selection, not a Leaf |

## Open questions

None.

## Reference evidence

- [F-31 durable session journal](F-31-durable-session-journal.md) — parent
  graph + durable selected cursor
- [F-37 materialized read models](F-37-materialized-read-models.md) —
  current state includes selected branch and session tree
- [F-09 session persistence](F-09-session-persistence.md) — residual
  mention of Leaf as the in-file branching model (superseded here)
- Session Tree feature: `packages/tui/docs/features/session-tree.md`
