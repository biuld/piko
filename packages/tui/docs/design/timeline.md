# Timeline Design

## Selected Feature

This design implements the `Timeline` feature contract in
`packages/tui/docs/features/timeline.md`.

The user-visible feature is a componentized conversation history stream in Slot
A. User messages, assistant messages, tool executions, notices, errors, and
summaries are distinct visible items with distinct behavior and styling. Live
updates mutate the existing message or tool item in place, and session rebuilds
produce the same message stream shape as live event playback.

## Responsibilities

Timeline owns:

- the ordered visible message stream for the active session branch
- live assistant and tool items that are still being updated
- lookup indexes used to update visible items by stable ids
- scroll position and pending-new counters
- local presentation state such as tool expansion
- rendering the message stream into Slot A

Timeline does not own:

- session persistence
- active branch selection
- hostd snapshots
- command routing
- orchestration state
- approval decisions
- layout slots outside Slot A
- global notifications

hostd remains authoritative for persisted user-visible session state. Timeline
is a TUI projection of hostd session state plus the live stream currently being
received.

## Architecture

Timeline remains one Slot A panel in the flat TUI layout. Internally, it owns a
sequence of timeline message components. These message components are not layout
slots; they are renderable units inside the Timeline panel.

```text
hostd Event / SessionSnapshot
        |
        v
AppState event handling
        |
        v
TimelineController
        |
        +--> TimelineDocument
        |       +--> message components
        |       +--> id indexes
        |       +--> live assistant pointer
        |
        +--> TimelineViewport
        |       +--> line scroll
        |       +--> pending-new count
        |
        v
TimelinePanel render(Slot A)
```

This keeps the existing Slot -> Panel boundary intact while letting each visible
message type own its state and rendering rules.

## Current hostd/protocol Contract

The first implementation must be based on the protocol shapes that hostd
already returns.

### Live Events

Timeline can consume these current `Event` variants directly:

- `UserMessageSubmitted { message_id, text, ... }`
- `MessageStart { message_id, role, ... }`
- `TextDelta { message_id, delta, ... }`
- `ThinkingDelta { message_id, delta, ... }`
- `MessageEnd { message_id, stop_reason, ... }`
- `AssistantMessageCompleted { message_id, message, ... }`
- `ToolStart { tool_call_id, tool_name, args, parent_message_id, ... }`
- `ToolEnd { tool_call_id, tool_name, result, is_error, ... }`
- `ToolResultCommitted { message_id, message, ... }`
- turn/task/session/auth events that become status, notice, or error state

The protocol already provides stable `message_id` for assistant streaming and
stable `tool_call_id` for tools. Timeline should use those ids instead of local
ids whenever they are present.

### Historical Snapshot Data

`SessionSnapshot.entries` contains `SessionTreeEntry` values. For Timeline,
the important variants are:

- `Message(MessageEntry)` with `Message::User`, `Message::Assistant`, or
  `Message::ToolResult`
- `ToolCall(ToolCallEntry)` for persisted tool-call declarations
- `Compaction(CompactionEntry)`
- `BranchSummary(BranchSummaryEntry)`
- `CustomMessage(CustomMessageEntry)`
- `ThinkingLevelChange`, `ModelChange`, `ActiveToolsChange`, `SessionInfo`,
  `Label`, and `Leaf` as compact notices or tree-only metadata depending on
  the feature doc

`Message::Assistant` already contains structured `AssistantContentBlock` values:

- `Text`
- `Thinking`
- `Image`

Historical assistant text, thinking, and image blocks can be reconstructed from
the assistant message. Historical tool-call declarations are reconstructed from
separate `ToolCallEntry` records so tool calls do not live inside assistant
content.

### Active Turn Scope

Each entry in `SessionSnapshot.active_turns` exposes:

- `turn_id`
- `agent_instance_id`
- `status`
- `assistant_text`
- `tool_calls: Vec<ToolCallSnapshot>`

In current hostd state construction, `assistant_text` is empty and
`tool_calls` is empty. `ToolCallSnapshot` also lacks args and parent message id.

This does not block the selected Timeline feature. The current TUI starts hostd
as a child process over stdio, so there is no supported reconnect flow where a
new TUI attaches to a still-running turn and needs to restore in-flight
assistant/tool content from snapshot.

For this feature, `active_turns` is status metadata only. In-flight Timeline
content comes from live streaming events received by the current TUI process.

## Core Model

Timeline should move from a string-oriented entry list to a typed document.

```rust
pub struct Timeline {
    document: TimelineDocument,
    viewport: TimelineViewport,
    presentation: TimelinePresentation,
}

pub struct TimelineDocument {
    components: VecDeque<TimelineComponent>,
    index: TimelineIndex,
    live_assistant: Option<ComponentId>,
}

pub enum TimelineComponent {
    User(UserMessageComponent),
    Assistant(AssistantMessageComponent),
    Tool(ToolExecutionComponent),
    Notice(NoticeComponent),
    Error(ErrorComponent),
    Summary(SummaryComponent),
    Custom(CustomMessageComponent),
}
```

`TimelineComponent` is a Timeline-internal message unit. It should not be
confused with reusable TUI components under the shared component layer.

Component ids are stable within the active rendered branch:

```rust
pub enum ComponentId {
    MessageId(String),
    ToolCallId(String),
    SessionEntryId(String),
    Local(u64),
}
```

Use durable ids from protocol/session data when available. Use local ids only
for transient visible items that have not yet been reconciled with durable
session state.

## Component Contract

Every timeline message component should be able to produce a line-oriented
render block for a given width and presentation state.

```rust
pub trait TimelineMessage {
    fn id(&self) -> ComponentId;
    fn kind(&self) -> TimelineKind;
    fn render_block(&self, ctx: &TimelineRenderContext) -> TimelineRenderBlock;
    fn set_expanded(&mut self, expanded: bool);
    fn is_expandable(&self) -> bool;
}

pub struct TimelineRenderBlock {
    id: ComponentId,
    lines: Vec<Line<'static>>,
    background: Option<Color>,
    top_spacing: u16,
    bottom_spacing: u16,
}
```

Render blocks are measured and clipped before ratatui rendering. This makes
Timeline scroll by rendered lines rather than by entry count.

## Message Components

### UserMessageComponent

Represents a submitted user prompt.

State:

- message id when available
- prompt text
- optional attachment or reference summaries

Rendering:

- user-message background with horizontal and vertical padding
- submitted content styled with the user message text token
- no visible `user` role label
- visually distinct from assistant output

Updates:

- normally immutable after insertion
- rebuilt from snapshots

### AssistantMessageComponent

Represents finalized or streaming assistant output.

State:

- message id or active turn id
- ordered content blocks
- streaming/finalized state
- stop reason and error text when user-visible

Assistant content blocks should distinguish:

- normal assistant text
- thinking text
- tool-call declarations

Updates:

- text deltas append to the current text block
- thinking deltas append to the current thinking block
- completion finalizes the component in place
- final assistant messages may replace accumulated draft content while
  preserving the component position
- aborts and errors attach stop reason state instead of becoming unrelated rows

The current `[thinking]` prefix should be treated as a compatibility bridge,
not the target representation.

### ToolExecutionComponent

Represents one tool call.

State:

- tool call id
- tool name
- parent message id when available
- args, preferably structured
- partial result
- final result
- status: pending, running, completed, failed, cancelled

Updates:

- tool-call discovery creates or updates a component by tool call id
- tool start marks the component running and updates args
- tool update mutates partial result
- tool end or committed result mutates final result and status
- turn failure marks unresolved related tools failed or cancelled

Rendering:

- pending/running/success/error backgrounds
- title styled with the tool title token
- output styled with the tool output token
- collapsed preview by default
- expanded details for args, parent message, partial result, final result, and
  error output

Tool updates must not append duplicate entries for the same tool call id.

### NoticeComponent

Represents durable system/session notices that belong in Timeline.

Examples:

- session opened
- session compacted
- branch navigation completed
- auth state changed

Transient progress should remain in AgentPanel, NotificationRow, or BottomBar.

Adjacent status-like notices may coalesce when no non-notice component was
appended between them.

### ErrorComponent

Represents user-visible errors that should remain in conversation history.

Rendering should put the concise error first. Expanded details can be added
later if needed.

### SummaryComponent And CustomMessageComponent

Compaction summaries, branch summaries, skill invocations, and future custom
messages should have dedicated component variants instead of being flattened
into generic notices.

Custom rendering is reserved for a later extension point. It should not grant
extensions direct access to layout slots.

## Live Event Mapping

### UserMessageSubmitted

Append or update a user message component keyed by `message_id`.

### TurnStarted

Do not append a generic Timeline item. Active-turn status belongs outside
Timeline.

### MessageStart

For assistant role, create a live assistant component keyed by `message_id`.
For user role, normally no Timeline change is needed because
`UserMessageSubmitted` carries the visible text. Other roles should be ignored
unless protocol adds visible content for them.

### TextDelta And ThinkingDelta

Find or create the assistant component keyed by `message_id`. Append text
deltas to text blocks and thinking deltas to thinking blocks. Keep the visible
component in one stable position.

### MessageEnd And AssistantMessageCompleted

`MessageEnd` records stop reason on the live assistant component and may mark
it finalized for streaming display. `AssistantMessageCompleted` carries the
authoritative `Message::Assistant`; replace the accumulated component content
with that structured message while preserving position and `message_id`.

### ToolStart

Upsert a tool component by `tool_call_id`.

If a matching `ToolCallCommitted` event was already seen, update that component
with execution state. Otherwise append the tool component near the live
assistant item. Use `parent_message_id` to associate the tool with its assistant
message when present.

### ToolUpdate

There is no current protocol event for streaming partial tool output. Keep this
as a reserved design point. Do not implement partial tool rendering until hostd
or orchd emits a distinct partial update event.

### ToolEnd And ToolResultCommitted

Update final result and status on the existing tool component. If no component
exists, create one from the result message and mark it final. `ToolEnd` carries
JSON `result`; `ToolResultCommitted` carries the persisted `Message::ToolResult`
with content blocks, details, and optional `is_error`.

### TurnFailed, TurnCancelled, Or Abort

Finalize live assistant state with a user-visible stop reason when appropriate.
Mark unresolved tools related to the turn failed or cancelled so Timeline does
not show permanently running work.

## Snapshot Rebuild

Opening a session, applying a state snapshot, navigating the tree, completing
compaction, or reloading should rebuild Timeline from authoritative session
state.

Rules:

1. Clear the document, indexes, and live assistant pointer.
2. Walk active branch entries in order.
3. Create user, assistant, notice, error, summary, and custom components from
   session entries.
4. For `Message::Assistant`, preserve structured `AssistantContentBlock`
   values. Create assistant components keyed by the `MessageEntry.id`.
5. When `ToolCallEntry` records appear, create tool components keyed by tool
   call id and store args/name from the entry.
6. When later `Message::ToolResult` entries appear, update matching tool
   components with result content, details, and error state.
7. Keep unresolved historical tool components indexed so later live events can
   update them.
8. Treat `snapshot.active_turns` as status-only. Rich in-flight assistant/tool
   restore is out of scope for this feature.
9. Show latest content after session open or rebuild unless the user explicitly
   had a preserved viewport position.

Snapshot rebuild and live event playback must converge to the same visible
Timeline for completed/persisted session state.

## Scrolling

Timeline scrolling is line-aware and is based on an internal `ScrollViewport`
plus ratatui's native scroll primitives.

```rust
pub struct TimelineViewport {
    offset_from_bottom: usize,
    pending_new_items: usize,
    content_height: usize,
    viewport_height: usize,
}
```

Render flow:

1. Build render blocks for all visible components at the current width.
2. Flatten blocks into measured lines while preserving component boundaries.
3. Update `ScrollViewport` with content height and viewport height.
4. Clamp `offset_from_bottom` to the current maximum scroll.
5. Convert bottom-origin offset into ratatui's top-origin paragraph scroll.
6. Render Slot A with `Paragraph::scroll`.
7. Render a right-side `Scrollbar` when content exceeds the viewport.

When at the bottom, new content keeps the viewport pinned to the newest line.
When scrolled up, new content preserves the user's current visual position and
increments the pending-new component count.

The pending-new hint counts components, not raw lines.

When Timeline is scrolled away from latest content, the Editor should not set
the terminal cursor. This prevents the input cursor from appearing over the
conversation while the user is inspecting older Timeline content.

## Presentation State

Initial presentation state can stay local to the TUI:

```rust
pub struct TimelinePresentation {
    tools_expanded: bool,
    thinking_visible: bool,
    output_padding: u16,
}
```

Promotion to `[tui]` config should happen only when a setting is clearly a
cross-session user preference.

## Ownership Boundaries

### protocol

Protocol carries structured events and snapshot data. It should not carry TUI
component state, scroll state, expansion state, or render hints.

Current protocol already supports historical assistant thinking and tool-call
declarations through `ContentBlock`. It also supports live text/thinking deltas
and tool start/end events.

Known gaps:

- no live partial tool-output event
- image rendering is represented in historical `ContentBlock::Image`, but the
  first Timeline implementation can leave image display out of scope

### hostd

hostd owns session storage, snapshots, active branch state, pending approvals,
and persisted TUI config blobs. It should not know about Timeline message
components.

### tui

TUI owns Timeline projection, indexes, local presentation state, viewport
state, and rendering.

The app event layer translates hostd events into TimelineController operations.
Timeline components do not send hostd commands directly.

## Migration Plan

### Phase 1: Message Components With Current Behavior

Replace the string-oriented `TimelineEntry` model with internal message
components for user, assistant, tool, notice, and error items while preserving
current visible behavior.

### Phase 2: Line-Aware Render Blocks

Introduce render blocks and line-aware clipping. Remove the current visible
item calculation that assumes roughly two rows per entry.

### Phase 3: Structured Assistant Content

Represent assistant text, thinking, stop reason, and tool-call declarations as
separate assistant content blocks. Historical data can use existing
`Message::Assistant.content`; live data can use `TextDelta`, `ThinkingDelta`,
`MessageEnd`, and `AssistantMessageCompleted`.

### Phase 4: Snapshot Parity

Make snapshot rebuild and live event playback produce equivalent component
streams for persisted entries, including unresolved historical tool calls.

### Phase 5: Richer Presentation

Add persisted thinking visibility, output padding, partial tool output display,
image-aware tool output, or custom renderers only after the core component
stream is stable.

## Testing Strategy

State transition tests:

- user, assistant, and tool components append in correct order
- text and thinking deltas mutate one live assistant component
- tool start, update, end, and committed results mutate one tool component by id
- unresolved historical tools remain indexed after snapshot rebuild
- current `active_turn` snapshot is treated as status-only
- failed or cancelled turns finalize unresolved live components
- notice coalescing only happens for allowed notice kinds
- bottom-pinned scroll stays pinned when content arrives
- scrolled-up viewport preserves visual position and records new items

Render tests:

- user message background
- assistant text versus thinking presentation
- tool collapsed and expanded states
- error styling
- variable-height clipping

Cross-crate tests are needed when protocol or hostd snapshot shapes change.

## Open Decisions

- Whether thinking visibility starts local or persisted under `[tui]`.
- Which notice kinds should coalesce.
- Whether partial tool output is part of the first implementation slice.
- Whether custom message rendering is near-term or only reserved by the type
  model.
