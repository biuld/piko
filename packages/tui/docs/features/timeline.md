# Timeline

> Parent UX: [ui-ux.md](./ui-ux.md) (shell IA, stream projection *principles*,
> information duties). This PRD is the **behavior contract** for the
> conversation stream—not a layout design for each projected kind.

## Overview

Timeline is the main conversation history area in the Chat layout. It shows the
active session branch as a message stream, including submitted user prompts,
assistant responses, tool executions, session notices, errors, and summaries.

Message kinds are visually distinct. **How each kind is laid out** (row zones,
wrap, per-tool body structure) follows the parent UX stream principles and
lives in presenters; it is not specified here type-by-type.

## Layout

Timeline’s place in the **Chat shell** (not per-message typesetting):

- Top elastic area of the Chat layout (above dock + chrome).
- Replaced by full-screen overlays (session list, tree, status, …).
- Remains visible when partial overlays replace the Editor.
- No enclosing box by default; subtle separators or status hints when useful.

## Behavior / interactions

Timeline displays the active session branch in chronological order. The newest
content appears at the bottom.

Submitted user prompts appear only after the server confirms them. Pressing
Enter clears the accepted editor input and may show turn status immediately,
but it does not create a temporary duplicate prompt in Timeline.

**What appears (projection duties):**

- User prompts: submitted blocks, no role label; only after server accept.
- Assistant output: no role heading; thinking quieter than answer text; may
  stream then finalize in place.
- Consecutive model steps are separated by one muted, non-interactive
  horizontal divider. The first visible model step has no leading divider;
  thinking and tool components remain on the side of the boundary where they
  were committed.
- Optional protocol `timestamp` on user/assistant: show local time when present
  (layout per parent stream principles, not specified here).
- Tools: separate cards with status-aware presentation; expand for typed detail.
- Notices/errors/facts: compact, not mistaken for assistant prose.
- Model / thinking / tool-set changes keep durable entry ids as fact rows;
  compaction and branch summaries use summary components; displayable custom
  messages use a custom component. Label, session-info, leaf, and non-display
  custom metadata do not enter Timeline.
- Fenced code: readable; syntax color when language is known; no decorative
  code frame as the default.

When live events update an existing assistant message or tool execution, the
existing visible item changes in place. The Timeline should not append duplicate
rows for every streaming text delta, tool start, or final tool result.

Assistant output may appear progressively while it is generated. When the
server confirms the complete message, that content replaces the temporary
draft. Missing or late streaming updates cannot change the confirmed message.
Messages confirmed by the server retain their task-local conversation order
even if confirmations arrive at the UI in a different order.

When a session is opened, reloaded, recovered, or navigated through the session
tree, Timeline rebuilds from the authoritative active session branch and
presents the same message stream shape as live updates would have produced.
Background compaction does not clear or rebuild the visible live Timeline.

The presentation consumes the canonical `piko-client-core` Timeline
projection. It does not maintain a second interpretation of stream patches or
committed transcript entries. Session-scoped facts are visible in every agent
view of the active branch; switching agents cannot accidentally move or hide a
fact based on which agent was selected when it arrived.

Switching agents shows that task's conversation. Returning to a previously
viewed task restores its confirmed messages and any current live draft without
mixing messages from another task.

Scrolling:

- PageUp scrolls up through older content.
- PageDown scrolls down toward newer content.
- Up and Down scroll by a smaller amount when the Editor does not use them for
  suggestions.
- Jump latest returns to the newest content.
- When already at the bottom, new content keeps Timeline pinned to the latest
  message.
- When scrolled up, new content does not move the user's view unexpectedly; a
  new-item hint indicates that newer content arrived.

Tool details (**behavior** only; per-tool layout is code + [ui-ux](./ui-ux.md)
stream principles):

- Collapsed by default; expand shows typed detail (not raw wire JSON).
  Live agent **todo list** is the `/todo` overlay ([todo-list.md](./todo-list.md) /
  F-27), not a force-expanded Timeline card. `todo_*` tools remain audit
  history; once the strip ships, checklist bodies need not stay open when
  collapsed.
- Title is scannable (name + short summary); status/exit/duration/tokens may
  appear as chrome. Prefer real result `usage` for tokens when present;
  otherwise a payload-size heuristic is fine. Call/parent ids are not chrome.
- Shell exit semantics belong with command outcome, not only generic tool-ok.
- Each block owns expand state independently; activate toggles that block only;
  hit target is the title/scan row.
- Expand state is in-memory for the session (kept across agent switches;
  cleared on projection rebuild).
- Failed/cancelled turns finalize still-running tools for that turn.

Thinking content:

- Assistant thinking is visually separate from normal assistant text.
- Thinking may be shown, hidden, or condensed depending on Timeline
  presentation settings.
- Visible thinking uses a fixed one-row live/completed summary and opens its
  complete content in the centered
  [Timeline Thought Inspector](timeline-thought-inspector.md).

## Configuration

Timeline tool expansion is transient per-session, per-tool presentation state.
It is not a global preference and is not stored under `[tui]` settings.

Settings that users expect to persist across sessions may later live under the
TUI configuration namespace. Candidate preferences include thinking visibility,
output padding, and richer tool-output display options that do not replace
individual tool-block state.

Timeline key bindings use the existing timeline action namespace:

| Binding ID | Default |
|------------|---------|
| `tui.timeline.pageUp` | PageUp |
| `tui.timeline.pageDown` | PageDown |
| `tui.timeline.up` | Up |
| `tui.timeline.down` | Down |
| `tui.timeline.jumpLatest` | configurable |

## Non-goals

- Timeline does not own session persistence or branch selection.
- Timeline does not decide whether a turn is running; active turn status belongs
  in surrounding status surfaces.
- Timeline does not create ad hoc floating UI. Thought detail is mounted
  through the shared modal surface system defined by the
  [Timeline Thought Inspector](timeline-thought-inspector.md).
- Timeline does not expose custom extension renderers in the first version.
- Timeline does not provide horizontal scrolling for long code lines.
- Timeline does not require image-capable tool output in the first version.
- Timeline does not require partial tool-output streaming in the first version.
- Timeline does not make every transient progress update a durable transcript
  entry.
- Title token estimates are **not** provider-billable per-tool accounting for
  every tool. Real usage is used when embedded in the tool result (e.g. spawn
  reports); otherwise the estimate is a coarse result-size heuristic.
- Column widths use `unicode-width` only — no locale-specific “Ambiguous = 2”
  override for title reservation.
