# Session Tree

## Overview

Session Tree is a full-screen navigator for the current session history. It
shows the session as a branchable tree instead of only the active conversation
path, so users can inspect earlier turns, switch to another point, and continue
without creating a separate session file.

Use Session Tree when alternatives should remain together in one session. Use
session fork or clone workflows when the result should become a separate
session.

## Layout

Session Tree replaces the main chat area as a full-screen overlay while leaving
the BottomBar visible.

The panel contains a bordered full-screen tree list. The title line identifies
the view, shows the selected row counter, active filter flags, search text when
present, and the primary confirm/cancel hints.

Tree rows use indentation and branch connectors to show the visible tree
structure. The visual structure follows pi's tree selector:

- The full session history is a parent/child tree rooted at the first visible
  session entries.
- Filter and search produce a visible subset of entries.
- When a filtered view hides intermediate entries, each visible entry attaches
  to its nearest visible ancestor.
- Branch connectors are drawn from visible sibling relationships, not from
  active-path state.
- A visible parent with two or more visible children draws a branch point. The
  children use `├` and `└` connectors, and descendants keep the appropriate
  vertical gutter while the sibling branch remains open.
- A visible parent with exactly one visible child stays flat. Single-child
  chains do not draw `├` or `└`, even when the child is outside the active
  path.
- Indentation increases at branch points and for the first generation after a
  branch. It does not increase for every parent/child edge.
- At each branch point, the child subtree that contains the current active leaf
  is shown before sibling subtrees.

The current active path is a separate visual concept from the branch
connectors. It is marked with an accent dot before the entry text for entries
on the path from the visible active leaf back to the root. Rows outside the
active path do not show that dot. A missing dot means "not on the current active
path"; it does not by itself create a branch connector. The active leaf, or the
nearest visible ancestor when the leaf is hidden by the active filter, is
selected by default when the panel opens.

Rows show a concise entry preview. User messages, assistant messages, tool
results, shell executions, compactions, branch summaries, custom messages,
model changes, thinking-level changes, labels, and session-title changes should
be distinguishable at a glance.

## Behavior / interactions

Session Tree opens from the tree command, the tree keybinding, or double Escape
when the editor is empty. If the session has no entries, the user sees a short
status message instead of an empty navigator.

Navigation:

| Key | Action |
|-----|--------|
| Up / Down | Move selection through visible entries |
| Left / Right | Move by one page |
| Enter | Navigate to the selected entry |
| Escape / Ctrl+C | Cancel and return to chat |

Search:

- Typing printable characters filters the visible rows.
- Backspace removes the last search character.
- Escape clears the search when search text is present.
- Escape cancels the panel when search text is empty.
- Search matches visible entry text, labels, roles, tool names, commands, and
  other concise entry metadata.

Tree interpretation:

- `•` means the row is on the currently active path.
- A row without `•` is another reachable point in the session tree, but it only
  receives a branch connector when it has a visible sibling under the same
  visible parent.
- If the active path has one visible child below the active leaf, pi presents
  that child as a flat continuation rather than drawing a branch connector.
- If the active path and a non-active path diverge under the same visible
  parent, the divergence must be visible with `├`/`└` connectors and any needed
  vertical gutter.

Filter modes:

| Mode | Behavior |
|------|----------|
| Default | Hide bookkeeping entries such as labels, custom metadata, model changes, thinking-level changes, and session-title changes |
| No tools | Use the default view and also hide tool result entries |
| User only | Show only user-authored message entries |
| Labeled only | Show only entries with labels |
| All | Show every session entry |

Filter shortcuts:

| Key | Action |
|-----|--------|
| Ctrl+D | Reset to default filter |
| Ctrl+T | Toggle no-tools filter |
| Ctrl+U | Toggle user-only filter |
| Ctrl+L | Toggle labeled-only filter |
| Ctrl+A | Toggle all-entries filter |
| Ctrl+O | Cycle filter forward |
| Shift+Ctrl+O | Cycle filter backward |
| Tab | Cycle filter forward |
| Shift+Tab | Cycle filter backward |

Branch movement and folding:

| Key | Action |
|-----|--------|
| Ctrl+Left / Alt+Left | Fold the current branch segment, or jump to the previous branch segment start |
| Ctrl+Right / Alt+Right | Unfold the current branch segment, or jump to the next branch segment start or branch end |

Folding hides descendants of the selected branch segment. Changing search or
filter mode clears folded state so the new result set remains predictable.

Labels:

| Key | Action |
|-----|--------|
| Shift+L | Edit or clear the selected entry label |
| Shift+T | Toggle label timestamps in the tree |

Labels are shown inline before the entry preview. Empty label input removes the
label. Label timestamps are hidden by default and can be shown temporarily
inside the panel.

Selection behavior:

- Selecting the current leaf is a no-op and returns to chat.
- Selecting a user message moves the session position to that message's parent
  and places the selected message text in the editor, ready to edit and submit
  as a new branch.
- Selecting a custom message moves the session position to that message's
  parent and places the selected custom message text in the editor.
- Selecting an assistant message, tool result, compaction, branch summary, or
  other non-user entry moves the session position to that entry and leaves the
  editor empty.
- Selecting the root user message resets the active position to before the
  first message and places the original prompt in the editor.

When navigation would abandon entries on the current active branch, a bottom
confirmation bar opens inside Session Tree asking whether to preserve those
abandoned entries as a branch summary. The tree stays visible above the bar and
the selected row is preserved. The abandoned entries are the current active
branch entries after the deepest common ancestor with the actual navigation
target. Selecting a user or custom message uses that message's parent as the
actual navigation target. Moving forward into a descendant, or selecting a user
message whose parent is the current leaf, does not prompt because no
current-branch entries are abandoned.

The summary choices are no summary, default summary, or summary with custom
focus instructions. Cancelling the summary choice closes the bottom bar and
returns to the tree with the same selected entry.

If branch summarization is running, Escape cancels summarization and returns the
user to tree navigation. Completed navigation rebuilds the Timeline from the
new active branch and returns focus to chat.

## Configuration

Session Tree uses the TUI keybinding system for all shortcuts. The relevant
binding IDs are:

| Binding ID | Default |
|------------|---------|
| `app.session.tree` | configurable |
| `app.tree.foldOrUp` | Ctrl+Left, Alt+Left |
| `app.tree.unfoldOrDown` | Ctrl+Right, Alt+Right |
| `app.tree.editLabel` | Shift+L |
| `app.tree.toggleLabelTimestamp` | Shift+T |
| `app.tree.filter.default` | Ctrl+D |
| `app.tree.filter.noTools` | Ctrl+T |
| `app.tree.filter.userOnly` | Ctrl+U |
| `app.tree.filter.labeledOnly` | Ctrl+L |
| `app.tree.filter.all` | Ctrl+A |
| `app.tree.filter.cycleForward` | Ctrl+O |
| `app.tree.filter.cycleBackward` | Shift+Ctrl+O |

The default tree filter is user-configurable. The default mode is Default.

Search text, selection position, folded branches, and label timestamp display
are transient panel state and do not persist after the panel closes.

## Non-goals

- Session Tree is not a filesystem tree.
- Session Tree does not create a new session file; use fork or clone for that.
- Session Tree does not edit message history in place.
- Session Tree does not run a model turn by itself after navigation.
- Session Tree does not replace the Timeline; it chooses the active branch that
  Timeline displays.
- Session Tree does not expose extension-specific custom renderers in the first
  version.
