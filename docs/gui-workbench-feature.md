# GUI Workbench

> Status: implemented feature contract; automated gates pass and native M4 UX validation remains open

## Overview

The piko GUI Workbench is a macOS-first desktop client for the same hostd
Sessions, Agents, Turns, transcripts, approvals, and user interactions exposed
by the existing TUI. It preserves piko's conversation-first workflow while
using desktop-native layout, pointer interaction, scrolling, typography, and
overlays.

The first release provides one complete path: choose or create a Session, read
its transcript, address a concrete Agent, submit and cancel a Turn, and resolve
approval or interaction prompts.

## Default Layout

```text
┌──────────────────┬──────────────────────────────────────┬───────────────────┐
│ Sessions         │                                      │ Agent Tree        │
│                  │                                      │ ● Main            │
│ Search           │        Conversation Timeline         │ ├─ Researcher     │
│ + New Session    │                                      │ └─ Reviewer       │
│                  │                                      ├───────────────────┤
│ Folder groups    │                                      │ Tree              │
│  ● alpha         │                                      │ ● current leaf    │
│    myapp         ├──────────────────────────────────────┤ ├─ prompt         │
│    other         │ Activity: 1 approval · 2 running     │ └─ branch         │
│                  ├──────────────────────────────────────┤                   │
│                  │ Main ▾ · Model ▾ · Think ▾    Send   │                   │
│                  │ Multi-line Composer                  │                   │
│                  ├──────────────────────────────────────┤                   │
│                  │ connected · context 12k/200k · $0.42 │                   │
└──────────────────┴──────────────────────────────────────┴───────────────────┘
```

Wide windows show the Sessions island on the left and, when open, Agents above
Tree on the right. Those right-hand islands share a column width
and a vertical split. Both outer docks are independently hideable and
resizable. The center Workbench remains fully usable as a single column and
never depends on either side.

## Island isolation and focus

Each Workbench island (Sessions, Timeline, Composer, Agents, Tree) is an
independent GPUI Entity: scroll, hover, and local UI updates re-render that
island only. Cross-island actions use directed `IslandMsg` routing through
chrome (`DesktopApp`), not a broadcast rebuild of the whole Workbench.

Keyboard focus is island-owned:

- Tab / Shift+Tab cycle visible islands (StatusBar is not focusable).
- `cmd-l` focuses the Composer island and its input.
- The focused island shows the theme `ring` on its shell.

There is no persistent center header. The center starts directly with the
Timeline. Session and cwd context belong to Sessions and the native
window title; the Composer identifies its target Agent and submission settings.

## Session Entry

- The left Sessions island lists all Sessions globally, grouped by working
  directory and sorted alphabetically.
- Starting with a requested Session opens that Session and shows a loading
  state until its complete view is available.
- Starting without a requested Session shows the global Sessions list and an
  action to create a new Session (create still uses the process cwd).
- Selecting a Session opens it in the same Workbench window.
- While a different Session is opening, the list distinguishes the live
  Session from the pending target.
- Creating a Session enters the Workbench immediately, but conversation state
  remains loading until the Session is ready.
- Failed open or create operations preserve the previous live Session when
  possible and show an actionable error.
- Local Composer drafts may be retained per Session within the current window.

## Agent Tree

The Agents island is the primary Agent selector and runtime overview for the
visible Session.

- It shows the durable AgentInstance parent/child hierarchy, not a flat list of
  transient model runs.
- Each row favors Agent display name and role, with compact indicators for
  lifecycle, active Turn, queued work, unread activity, or failure when known.
- Selecting an Agent changes the center Timeline, Composer target, Turn status,
  and lower Tree together; it does not create or start work.
- The selected Agent, actively running Agents, and Agents needing attention are
  visually distinct states.
- Expansion, selection focus, and panel scroll are window-local UI state. Agent
  identity and hierarchy come from the reconciled Session projection.

## Tree

The Tree island is a compact structural map of the selected Agent's
conversation. It is closer to an editor's symbol tree or minimap than to a
second transcript.

- It shows the selected Agent's conversation branch hierarchy, current leaf,
  active branch, labels, and compact summaries of meaningful entries.
- Message nodes favor a label or first-line summary; tool nodes favor tool name
  and status. Full content remains in the Timeline.
- The current leaf and active branch are always visually distinct.
- Branches can be expanded and collapsed without changing Session authority.
- Selecting an entry on the active branch scrolls the Timeline to it.
- Selecting an entry outside the active branch previews its position and
  exposes an explicit branch-switch action. A single click never silently
  rewrites the visible branch.
- A branch switch that requires summarization or confirmation opens the
  corresponding focused workflow before navigation.
- Selecting another Agent replaces the map with that Agent's structure and
  restores its window-local expansion and scroll state when available.
- Reconciliation replaces the map and realigns selection to the authoritative
  current leaf for the selected Agent.

## Timeline

- The Timeline shows the selected Agent's active branch in chronological order.
- User, assistant, tool, thinking, system, and error content have distinct but
  visually related presentation.
- Assistant Markdown and code blocks are readable at desktop widths. Rendered
  transcript selection/export is deferred; Composer selection remains native.
- Tool calls are compact by default and can be expanded to inspect arguments,
  results, and failure details.
- Realtime assistant output appears as an in-progress entry and becomes the
  committed message without duplication.
- The viewport follows new output only while the user is already near the end.
  Reading older content is never interrupted by incoming output.
- When detached, `cmd-j` reattaches follow and scrolls to the end (no in-panel
  Jump button — Timeline uses the same IslandPanel scrollbar as other islands).
- Switching Agents preserves each Agent's Timeline and scroll position for the
  current window.

## Activity Center

Activity Center sits in the center Workbench between Timeline and Composer. It
answers "what is happening or needs attention" at the point where the user
decides the next action. It is a compact operational surface, not a second
Timeline or an unbounded event log.

- Its collapsed header summarizes running Agents and tools, queued work, pending
  prompts, unread Agent reports, and warnings or errors.
- Unless explicitly collapsed by the user, it expands automatically for an
  approval, interaction, failure, disconnect, or other user-actionable
  condition, while ordinary progress stays compact.
- Activating an item selects its owning Agent when applicable and focuses or
  restores the corresponding approval, interaction, failed operation, or
  Timeline location.
- Prompt items remain until host resolution or reconciliation. Sending a
  response disables duplicate action but does not optimistically remove them.
- Short-lived success and informational feedback may remain a toast; durable
  conversation events remain in the Timeline. The Activity Center retains only
  current state and a small bounded set of actionable GUI notifications.
- When there is nothing notable, the panel contracts to a single quiet status
  row and yields vertical space to the Timeline.
- Expansion consumes Timeline height up to the configured cap. Composer and
  StatusBar remain fixed and usable; Activity never replaces either surface.

## Agent Selection and Turn State

- Agent Tree uses host-provided display names and status; Composer repeats only
  the selected target needed to understand the next submission.
- Selecting an Agent changes both the visible Timeline and the target of the
  next Turn.
- The active Turn indicator is scoped to the selected Agent.
- A running Turn exposes a separate Stop action without replacing the Composer,
  Timeline, or Send action.
- Agent identity, Turn state, and transcript content never use fabricated
  placeholders while a Session is loading.

## Composer

- The Composer starts at three lines, grows to a bounded multi-line height, and
  then scrolls internally.
- Activity and Composer share the full center-column width (not the Timeline
  reading-width cap) so they stay aligned as the window resizes.
- Its action row shows the selected target Agent plus model and thinking
  controls. These values affect the next submission and do not occupy a
  persistent Workbench header.
- The target is a compact selector such as `Main ▾` or `@Main`, not prose such
  as `To Main`.
- `Enter` submits non-empty text.
- `Shift+Enter` inserts a newline.
- `Command+.` requests cancellation of the selected Agent's active Turn.
- The Send action is disabled until a Session and target Agent are ready.
- Send and Stop are distinct controls. Send submits a new Turn and may queue it
  when the target is busy; Stop appears only while that Agent has a cancellable
  Turn.
- Submitted text is not inserted into the Timeline optimistically. It appears
  when hostd commits it.
- A rejected submission restores the submitted text when doing so would not
  overwrite newer user input.
- The Composer retains its local draft while Session creation or hydration is
  in progress.

## Overlay Stack

Focused workflows appear above the Workbench with a dimmed backdrop. The
underlying Timeline remains visible but is not interactive.

### Approval

- An approval card describes the proposed tool and presents its structured
  arguments.
- Available decisions are Accept once, Accept for Session, Accept for
  Workspace, Accept permanently, and Decline.
- Clicking the backdrop or pressing Escape does not implicitly approve or
  decline.
- After a decision is sent, controls are disabled until hostd resolves or
  rejects it.
- Multiple approvals are serialized; the card shows how many prompts remain.

### User interaction

- Questions, choices, optional inline text, and confirmation use a focused
  desktop form.
- Multiple questions expose clear step or tab navigation.
- Cancelling the form sends an explicit cancellation response; it does not only
  hide the UI.
- Approval and user interaction share visual language but retain distinct
  labels and outcomes.

### Other overlays

- Session selection may use a side sheet or focused dialog while retaining the
  Workbench underneath.
- Palette and Settings overlays are later additions and do not occupy permanent
  Workbench space.

## Status and Notifications

- The bottom StatusBar is a stable, read-only environment summary: host
  connection, context usage, and cumulative cost. It does not show Turn, queue,
  prompt, tool, or notification items owned by Activity Center.
- Session Sidebar and the native title expose project context. When Session
  Sidebar is hidden, StatusBar adds an abbreviated cwd so the single-column
  Workbench remains oriented without reintroducing a center header.
- Current Agent belongs to Composer target and Agent Tree, not StatusBar.
- Model and thinking controls belong to Composer because they affect the next
  submission; StatusBar may show context consumption but does not duplicate
  those controls.
- Activity Center owns inspectable live status and actionable attention.
- Toast notifications carry short-lived command feedback. Warnings and errors
  that still require action also remain inspectable in Activity Center.

## Loading and Spinners

Spinners are local progress indicators and never stand in for a general app
status:

- connecting or reconnecting animates the connection item in StatusBar;
- opening, creating, or hydrating marks the pending Session row and uses a
  Timeline skeleton only when no previous live Session can remain visible;
- a running Agent animates its Agent Tree row and has a corresponding Activity
  item;
- a running tool animates its Timeline card and corresponding Activity item;
- submit and prompt-response buttons show progress only while their command is
  awaiting an outcome and remain protected from duplicate activation.

No full-window spinner is used after the window opens, and no spinner infers
authority before the matching host event or reconciliation.

## Responsive Sidebars

- Wide windows show Sessions and the right column (Agents + Tree)
  by default. Agents and Tree share column width through a vertically resizable
  split; each island has its own open preference (UI still toggles them together).
- Activity Center remains between Timeline and Composer at every window width.
  It has a compact bounded height and can be collapsed to its summary row.
- Medium windows keep Sessions docked and collapse Agents/Tree behind a toolbar
  action (`cmd-i`).
- Narrow windows undock Sessions, Agents, and Map. They remain available as
  temporary sheets above the Workbench; the right sheet keeps Agents above Map
  in the same vertical split as the docked column.
- Outer islands are resizable and hideable. Closing Sessions and both right
  islands returns to the single-column Workbench with no loss of functionality.
- Island visibility, size, expansion, and selection are presentation state.
- Visibility and size persist under hostd's opaque `[gui]` namespace.

## Configuration

GUI-specific settings use the `[gui]` namespace stored by hostd. Panel sizes,
sidebar visibility, and reduced-motion preference persist there. Typography,
map expansion, focus, drafts, and per-Agent scroll/follow state remain
frontend-local until their controls and recovery semantics are stable.

Physical GUI shortcuts are frontend-specific even when their action names match
TUI command intent.

## Non-goals

- Pixel-for-pixel TUI reproduction
- Reusing the ratatui Slot A-E layout engine
- Modifying, replacing, or deprecating the TUI
- Direct GUI-to-orchd communication
- Reading or writing Session storage outside hostd
- Multiple windows or attaching multiple clients to one hostd process
- A freely nestable window-management system in the first release
- Complete migration of every TUI palette, selector, setting, and command
