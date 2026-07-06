# Session Tree Design

## Selected Feature

This design implements the `Session Tree` feature contract in
`packages/tui/docs/features/session-tree.md`.

The user-visible feature is a full-screen session history navigator. Users can
inspect the complete branch tree for the active session, filter and search
visible entries, fold branch segments, label entries, and navigate to a selected
entry without creating a separate session file.

## Responsibilities

TreePanel owns:

- transient panel state: selected row, search query, active filter mode, folded
  node ids, label editing input, and label timestamp visibility
- building a visible tree projection from `SessionSnapshot.entries`
- rendering the full-screen tree overlay
- local keyboard handling for tree navigation, search, filters, folding, and
  label editing

TreePanel does not own:

- durable session entries
- the active session leaf
- branch summary generation
- durable entry labels
- timeline reconstruction after navigation
- global command routing

hostd remains authoritative for durable user-visible session state. The TUI
requests mutations and then rebuilds from hostd snapshots or mutation-result
events.

## Placement And Focus

Session Tree is an `AppMode::Tree` full overlay. It replaces Slots A, B, C, and
D and leaves the BottomBar in Slot E.

Opening the tree pushes `AppMode::Tree` onto `FocusManager`. Closing the panel
pops focus back to the previous mode, usually Chat. Successful navigation clears
focus back to Chat after hostd acknowledges the mutation.

Input priority stays consistent with the rest of the TUI:

1. Global Escape/Enter handling.
2. TreePanel handles mode-specific keys while focused.
3. Editor fallback is not reached while TreePanel captures input.

Escape behavior is contextual:

- label input open: close label input
- search text present: clear search and unfolded state
- otherwise: close TreePanel

## Data Flow

Opening the tree uses the latest local `SessionSnapshot` already held by
`AppState`. If no active session exists, the TUI shows a status message and
stays in Chat.

```text
SessionSnapshot.entries + current_leaf_id
        |
        v
TreePanel::load(...)
        |
        v
TreeDocument
        |
        +--> full node index
        +--> label overlay index
        +--> parent/child maps
        +--> active path ids
        |
        v
VisibleTree
        |
        +--> filter
        +--> search
        +--> folded descendants removal
        +--> visible parent/child maps
        |
        v
TreePanel render
```

Navigation sends a command to hostd:

```text
TreePanel selected entry id
        |
        v
Command::SessionNavigate
        |
        v
hostd validates idle session and target entry
        |
        v
hostd appends leaf entry and optional branch_summary
        |
        v
hostd emits navigation result + SessionOpened/StateSnapshot
        |
        v
TUI applies editor text, rebuilds Timeline, reloads TreePanel
```

## Tree Model

TreePanel should move from a flat `FilterableList<TreeEntry>` to a tree-specific
model. The shared `FilterableList` remains useful for simple overlay lists, but
Session Tree needs tree connectors, active path marking, folded descendants,
nearest-visible ancestor selection, and branch-segment movement.

```rust
pub struct TreePanel {
    document: TreeDocument,
    visible: VisibleTree,
    selection: Option<EntryId>,
    search_query: String,
    filter_mode: TreeFilterMode,
    folded: HashSet<EntryId>,
    show_label_timestamps: bool,
    label_editor: Option<LabelEditorState>,
}
```

The document stores all durable entries plus derived maps:

```rust
pub struct TreeDocument {
    nodes: Vec<TreeNode>,
    by_id: HashMap<EntryId, usize>,
    children_by_parent: HashMap<Option<EntryId>, Vec<EntryId>>,
    labels_by_target: HashMap<EntryId, TreeLabel>,
    active_path: HashSet<EntryId>,
    current_leaf_id: Option<EntryId>,
}
```

`LabelEntry` is durable metadata, not a normal visible tree node in default
mode. While building `TreeDocument`, latest label entries are folded into
`labels_by_target`. The label entry itself remains available in All mode.

The visible projection stores rows after filters, search, and folding:

```rust
pub struct VisibleTree {
    rows: Vec<TreeRow>,
    parent_by_id: HashMap<EntryId, Option<EntryId>>,
    children_by_id: HashMap<Option<EntryId>, Vec<EntryId>>,
}
```

Rows contain render-ready structural data:

```rust
pub struct TreeRow {
    entry_id: EntryId,
    depth: usize,
    connector: ConnectorKind,
    gutters: Vec<Gutter>,
    is_current_leaf: bool,
    is_active_path: bool,
    is_folded: bool,
    label: Option<TreeLabel>,
    preview: TreePreview,
}
```

Single-child chains should remain visually compact. Indentation increases at
branch points and for the first generation after a branch, matching pi-mono's
behavior instead of blindly indenting every parent/child edge.

## Filtering And Search

Filter modes are:

- `Default`
- `NoTools`
- `UserOnly`
- `LabeledOnly`
- `All`

Default mode hides bookkeeping entries:

- labels
- custom metadata entries
- model changes
- thinking-level changes
- session-title changes

NoTools starts from Default and also hides tool result entries. UserOnly keeps
only user messages. LabeledOnly keeps entries with a label attached. All keeps
all entries.

Search runs after filter mode. It tokenizes the query on whitespace and keeps a
row only when all tokens are present in the searchable text. Searchable text
includes:

- labels
- message roles
- message text
- custom message type and text
- branch summary text
- compaction marker text
- tool names and concise arguments
- shell commands
- model and thinking-level values
- session-title text

Folding runs after search. If a node is folded, all visible descendants are
removed. Changing search or filter mode clears folded state because the
descendant set may have changed.

Selection preservation uses nearest-visible ancestor fallback. When the
currently selected entry disappears because of filter/search/folding, selection
walks up the full tree parent chain until it finds a visible ancestor. If none
exists, it selects the last visible row.

## Rendering

TreePanel renders as a full overlay with a compact title/status line and a
scrollable tree body:

```text
session tree [selected/visible] [flags] | filter: query | Enter confirm | Esc close
<tree rows>
```

Rows have a fixed cursor gutter. Connector and active-path markers stay before
the entry preview. The selected row is highlighted across its rendered text.
The active path marker is an accent dot before the entry text and is shown only
for entries on the active branch.

Visible row order follows pi-mono: when a subtree contains the current active
leaf, that subtree is displayed before sibling subtrees. This keeps the active
conversation path first at branch points.

Entry preview formatting should be concise:

- user: first text content
- assistant: first text content, aborted marker, or error message
- tool result: corresponding tool call summary where available
- shell execution: command preview
- custom message: custom type and first text content
- compaction: token count and summary marker
- branch summary: summary preview
- model/thinking/session metadata: compact notice text
- label entry in All mode: label value or cleared marker

Tool result rows should resolve the matching assistant tool call by tool call id
when possible so the row can show `[read: path]`, `[edit: path]`, `[bash:
command]`, or other concise call summaries.

## Input Handling

TreePanel handles these action groups:

- selection movement: up/down/page up/page down
- confirm/cancel
- search text insertion and backspace
- filter direct selection and cycling
- branch fold/unfold and segment movement
- label edit and label timestamp toggle

Tree-specific shortcuts should be routed only while `AppMode::Tree` is focused.
They must not steal editor shortcuts in Chat or session-list shortcuts in
SessionList.

Branch segment movement depends on the visible tree:

- `foldOrUp`: if the selected visible node is foldable and not folded, fold it;
  otherwise move to the previous branch segment start.
- `unfoldOrDown`: if the selected node is folded, unfold it; otherwise move to
  the next branch segment start or current branch end.

A node is foldable when it has visible children and is either a root or the
start of a visible branch segment.

## Protocol Contract

Current `Command::SessionNavigate { session_id, entry_id }` is not enough for
the full feature. It updates the leaf and emits a snapshot, but it does not
return editor text for selected user/custom messages and has no branch summary
options.

Extend the command:

```rust
SessionNavigate {
    command_id: CommandId,
    session_id: SessionId,
    entry_id: String,
    summarize: bool,
    custom_instructions: Option<String>,
}
```

Add a mutation result event before or alongside the follow-up snapshot:

```rust
SessionNavigated {
    session_id: SessionId,
    old_leaf_id: Option<String>,
    new_leaf_id: Option<String>,
    selected_entry_id: String,
    editor_text: Option<String>,
    summary_entry: Option<SessionTreeEntry>,
    timestamp: i64,
}
```

The result event lets TUI set editor text without re-deriving navigation
semantics from a changed snapshot. The follow-up snapshot remains necessary for
Timeline and TreePanel rebuilds.

For labels, add a dedicated command:

```rust
SessionSetLabel {
    command_id: CommandId,
    session_id: SessionId,
    entry_id: String,
    label: Option<String>,
}
```

hostd appends a `LabelEntry` targeting `entry_id` and emits a fresh snapshot.
The TUI should optimistically update TreePanel only after command acceptance, or
wait for the snapshot for simpler consistency.

## hostd Semantics

Navigation is a structural session mutation and must only run while the session
is idle. If a turn is active, hostd returns an error and TUI keeps the tree open
with a status message.

hostd computes navigation semantics:

- If the target entry is a user message, new leaf is the target's parent and
  `editor_text` is the selected message text.
- If the target entry is a custom message, new leaf is the target's parent and
  `editor_text` is the selected custom message text.
- Otherwise new leaf is the target entry id and `editor_text` is absent.

The root user-message case naturally sets new leaf to `None`, which resets the
conversation to before the first entry and returns the original prompt in
`editor_text`.

hostd persists navigation by appending a `LeafEntry` whose `target_id` is the
new leaf id. It should not set the leaf directly in memory without a durable
entry.

Branch summary is part of the navigation operation. hostd finds the common
ancestor between old and new positions, collects the abandoned branch entries,
and optionally generates a `BranchSummaryEntry` before emitting the navigation
result. If summarization is cancelled, hostd leaves the leaf unchanged.

## TUI Navigation Flow

When the user confirms a selected entry:

1. If selected entry is the current leaf, close TreePanel and show an
   already-at-this-point status.
2. Derive the actual navigation target using hostd semantics: user and custom
   message entries target their parent leaf; all other entries target
   themselves.
3. Collect abandoned entries using the same rule as hostd/pi-mono: find the
   deepest common ancestor between the current leaf path and the actual target
   path, then collect current-branch entries after that ancestor.
4. If the abandoned entry set is non-empty, open the summary-choice flow.
5. If the summary choice is cancelled, reopen or keep TreePanel with the same
   selection.
6. Send `SessionNavigate` with the chosen summary options.
7. On `SessionNavigated`, set editor text only when the editor is empty.
8. On the following snapshot, rebuild Timeline and TreePanel from hostd state.
9. Return focus to Chat and show a navigation status.

The summary-choice flow is a focused state inside the TreePanel full overlay.
It keeps the tree visible and replaces the tree footer with a bottom
confirmation bar. The focus stack still pushes a summary-prompt mode so its
keys are isolated from normal tree search/navigation, but layout placement
continues to occupy the same full tree slot instead of using the partial
overlay slot.

The bottom confirmation bar is attached to the TreePanel inner bottom edge. It
uses the full tree content width, reserves six rows for the normal choice
state, and reserves seven rows while custom instructions are active. The
content is padded horizontally and vertically enough to read as a bottom sheet
instead of a dense status footer. The tree list above it shrinks while the
prompt is open, preserving the selected entry and surrounding context.

It must not fire just because the selected visible row is off the active path;
it fires only when hostd would have non-empty abandoned entries to summarize.

## Configuration

Add TUI config for the default tree filter:

```rust
pub struct TreeConfig {
    pub filter_mode: TreeFilterMode,
}
```

`TuiConfig` gains `tree: TreeConfig`, with default `filter_mode = Default`.
TreePanel initializes from this config when opened. Changing filter mode inside
the panel is transient unless a later settings surface explicitly persists it.

Branch summary prompting is not purely TUI presentation. It affects durable
session mutation behavior and belongs with hostd/session navigation semantics.
The TUI derives whether a prompt is required from the same abandoned-entry
calculation hostd uses, then sends the user's summary choice on
`SessionNavigate`. A persisted branch-summary prompt preference is out of scope
for this version.

## Implementation Phases

Phase 1: Tree projection and navigation parity without branch summary.

- Replace the flat tree list with `TreeDocument` and `VisibleTree`.
- Add tree-specific rendering, search, filters, folding, and active path
  markers.
- Extend `SessionNavigate` to return `editor_text`.
- Implement user/custom message editor refill.
- Add focused tests for tree projection, filters, folding, and navigation
  semantics.

Phase 2: labels.

- Add `SessionSetLabel`.
- Fold latest label entries into target nodes.
- Add label edit input and label timestamp toggle.
- Add tests for label persistence and tree display.

Phase 3: branch summary prompt.

- Add summary options to `SessionNavigate`.
- Add the summary-choice panel flow.
- Integrate branch-summary generation/cancellation in hostd.
- Add tests for cancelled summary, no-summary navigation, and summary entry
  insertion.

## Validation

Focused TUI checks:

- tree projection unit tests for indentation, active path, current leaf, and
  multiple roots
- filter/search tests for all filter modes and nearest-visible selection
- folding tests for branch segment behavior
- input routing tests for tree shortcuts being scoped to `AppMode::Tree`

Cross-crate checks once protocol/hostd changes land:

- protocol serialization tests for new commands/events
- hostd navigation tests for user-message editor text, custom-message editor
  text, non-user navigation, root-message reset, and idle-only enforcement
- hostd label persistence tests
- branch summary navigation tests when Phase 3 lands

Run:

- `cargo fmt --all`
- `cargo test -p tui`
- `cargo test -p hostd`
- `cargo test -p piko-protocol`
- `cargo clippy --workspace --all-targets -- -D warnings`

Use `cargo test --workspace` when Phase 3 crosses compaction/orchestration
boundaries.

## Open Questions

- Should the default tree filter be persisted as `tui.tree.filter_mode`, or
  should tree filter preferences live with broader session/navigation settings?
- Should branch summary prompting be implemented before labels if navigation
  parity with pi-mono is the immediate priority?
- Should label timestamps use the label entry timestamp only, or also support a
  future explicit `labelTimestamp` field if pi-compatible storage requires it?
