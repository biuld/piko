# Timeline

## Overview

Timeline is the main conversation history area in the Chat layout. It shows the
active session branch as a message stream, including submitted user prompts,
assistant responses, tool executions, session notices, errors, and summaries.

Each visible message type has its own presentation. User prompts, assistant
output, tools, notices, and errors should be visually distinct instead of being
rendered as one generic text row style.

## Layout

Timeline occupies the top elastic area of the Chat layout.

- It sits above the AgentPanel, notifications, suggestions, Editor, and BottomBar.
- It is replaced by full-screen overlays such as the session list, tree, help,
  and status views.
- It remains visible when partial overlays replace the Editor.
- It has no enclosing box by default; it may use subtle separators or status
  hints when useful.

## Behavior / interactions

Timeline displays the active session branch in chronological order. The newest
content appears at the bottom.

Submitted user prompts appear only after the server confirms them. Pressing
Enter clears the accepted editor input and may show turn status immediately,
but it does not create a temporary duplicate prompt in Timeline.

User messages appear as submitted prompt blocks with a distinct background and
without a visible role label. Assistant messages appear as plain assistant
output without an `assistant` heading; thinking content is visually quieter than
normal answer text. Tool executions appear as separate padded blocks with
state-specific backgrounds for pending, completed, and failed work. Session
notices and errors use compact styles that do not look like normal assistant
output.

Fenced code blocks appear as unobstructed code without a decorative box or
generic title. When the fence names a recognized language, syntax colors make
keywords, strings, comments, and other language elements easier to scan. Code
without a language, code using an unknown language, and exceptionally large
blocks remain readable as plain text.

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

Tool details:

- Tool blocks have collapsed and expanded presentations.
- Collapsed mode shows the tool name, status, short id, and a concise preview.
- Expanded mode shows additional details such as arguments, parent message, and
  results when available.

Thinking content:

- Assistant thinking is visually separate from normal assistant text.
- Thinking may be shown, hidden, or condensed depending on Timeline
  presentation settings.

## Configuration

Initial Timeline behavior uses local TUI state for transient presentation, such
as whether tool details are expanded.

Settings that users expect to persist across sessions may later live under the
TUI configuration namespace. Candidate preferences include thinking visibility,
output padding, and richer tool-output display options.

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
- Timeline does not create floating UI.
- Timeline does not expose custom extension renderers in the first version.
- Timeline does not provide horizontal scrolling for long code lines.
- Timeline does not require image-capable tool output in the first version.
- Timeline does not require partial tool-output streaming in the first version.
- Timeline does not make every transient progress update a durable transcript
  entry.
