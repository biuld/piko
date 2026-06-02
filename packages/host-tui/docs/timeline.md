# Timeline System

Timeline is the subsystem that turns Host/Engine events and session transcript data into a scrollable, interactive message timeline.

Timeline owns streaming behavior, scroll anchoring, user scroll intervention, message grouping, message type rendering policy, and timeline-local interactions such as tool expansion.

## Goals

- Convert runtime/session events into stable timeline items.
- Render streaming assistant text incrementally without layout corruption.
- Keep auto-scroll-to-bottom while the user has not intervened.
- Stop auto-scroll when the user manually scrolls away from bottom.
- Resume auto-scroll when the user returns to bottom or explicitly jumps to latest.
- Define rendering behavior for every message/item type.
- Keep transcript/domain state separate from timeline view state.
- Make tool/thinking/summary expansion part of timeline state, not component-local state.

## State model

Timeline state should live under `ui.timeline` or `view.timeline`.

```ts
type TimelineAnchor =
  | "bottom"
  | "manual"
  | "item";

interface TuiTimelineState {
  items: TimelineItem[];
  anchor: TimelineAnchor;
  anchorItemId?: string;
  atBottom: boolean;
  userScrolled: boolean;
  pendingNewItems: number;
  selectedItemId?: string;
  expandedItemIds: Set<string>;
  collapsedToolCallIds: Set<string>;
  streamingItemId?: string;
}
```

`anchor` meaning:

- `bottom`: auto-follow new streaming output and new messages.
- `manual`: user has scrolled; do not force scroll.
- `item`: keep a specific item visible, useful for selected message or search result.

## Timeline items

Transcript messages are not the only thing that can appear in the timeline.

```ts
type TimelineItemKind =
  | "user-message"
  | "assistant-message"
  | "assistant-stream"
  | "tool-call"
  | "tool-result"
  | "thinking"
  | "branch-summary"
  | "compaction-summary"
  | "system-note"
  | "approval"
  | "notification-ref";

interface TimelineItem {
  id: string;
  kind: TimelineItemKind;
  role?: "user" | "assistant" | "tool" | "system";
  text?: string;
  createdAt?: number;
  messageId?: string;
  toolCallId?: string;
  parentId?: string;
  isStreaming?: boolean;
  isCollapsed?: boolean;
  severity?: "info" | "success" | "warning" | "error";
  data?: unknown;
}
```

Rules:

- Use stable ids. Do not regenerate ids on every render.
- Streaming item id stays stable for the active assistant response.
- Tool call/result items are addressable by `toolCallId`.
- Summary items are first-class timeline items, not styled plain text only.
- Notifications usually stay in the notification system; only create `notification-ref` when a notice is important enough to appear in transcript context.

## Streaming behavior

Streaming should be modeled as timeline updates:

1. User submits prompt.
2. User message item is appended.
3. Assistant stream item is created with stable id.
4. Text deltas append to the assistant stream item.
5. Tool call deltas create/update tool-call items.
6. Tool results update corresponding tool-result/tool-call item.
7. Final assistant message replaces or finalizes assistant stream item.
8. Stream state returns to idle.

Requirements:

- Do not create a new assistant item for every text delta.
- Do not force re-render of the entire timeline on every delta if avoidable.
- Preserve scroll position when user is in manual mode.
- If anchor is `bottom`, scroll to bottom after each render batch.
- If anchor is `manual`, increment `pendingNewItems` instead of jumping.
- Show a latest/new-output indicator when pending items exist.

## Scroll behavior

Timeline scroll is stateful and user-driven.

Default:

- Start with `anchor = "bottom"`.
- Auto-scroll during streaming while `atBottom === true`.
- Keep bottom anchored when new user/assistant/tool items arrive.

User intervention:

- If user scrolls up, set `anchor = "manual"` and `userScrolled = true`.
- While manual, new items do not change scroll offset.
- Show pending-new-items status or inline latest indicator.
- If user scrolls back to bottom, set `anchor = "bottom"` and clear pending count.
- Provide a keybinding/action to jump to latest.

Programmatic anchors:

- Selecting a message can set `anchor = "item"` with `anchorItemId`.
- Closing selection/search returns to previous anchor.
- Compaction/fork/tree navigation can anchor to relevant summary item.

## Rendering policy by item type

### User message

- Clearly labeled or visually distinct.
- Preserve pasted multi-line content.
- Avoid oversized labels.
- Separate from adjacent assistant/tool content with timeline separator.

### Assistant message

- Plain readable text.
- Streaming state may show subtle cursor/ellipsis only when useful.
- Do not collapse normal assistant text.

### Assistant stream

- Same visual style as assistant message.
- Stable item while streaming.
- If empty, show minimal muted placeholder.
- Finalize into assistant message without visual jump.

### Tool call/result

- Render through tool display registry.
- Show tool name, status, concise summary.
- Collapsed by default when long.
- Expansion state stored in timeline state.
- Running/error/success use semantic theme tokens.

### Thinking

- Render as subdued expandable content.
- Keep separate from assistant final text.
- Respect thinking visibility settings.

### Branch and compaction summaries

- Render as summary blocks with restrained accent.
- Do not use emoji/iconography unless supported consistently by theme and terminal.
- Must be visually separated from normal messages.

### Approval

- If interactive, it is a surface/focus concern.
- Timeline may show approval result after completion.

### System note

- Used for session-level lifecycle notes only when they belong in transcript context.
- Runtime warnings should usually be notifications, not timeline items.

## Separators and grouping

Message separation should be explicit but quiet.

Rules:

- Use full-width border/separator between major message groups.
- Do not put separators between tightly coupled tool call/result rows.
- Group assistant text with its immediate tool calls/results when visually useful.
- Branch/compaction summaries get their own separation.
- Avoid nested cards; timeline is a text flow.

## Timeline interactions

Timeline should support:

- scroll up/down
- page up/down
- jump latest
- select previous/next message if selection mode exists
- expand/collapse tool or thinking block
- copy message/tool output later

These interactions must route through `FocusManager`:

- editor owns focus normally
- timeline can receive focus when user enters scroll/selection mode
- while timeline focus is active, scroll keys affect timeline instead of editor
- `Esc` returns focus to editor

## Layout integration

Timeline receives a row budget from layout:

```ts
interface TimelineLayout {
  width: number;
  height: number;
  mode: "regular" | "compact" | "minimal";
}
```

Rules:

- Timeline height is the remaining area after status, editor, bottom bar, and active surface mounts.
- Timeline should not infer terminal dimensions directly from renderer.
- Message renderers receive width and mode.
- Long lines must wrap or truncate according to item policy.
- Tool blocks must not resize surrounding layout unpredictably.

## Renderer structure

Add:

```text
packages/host-tui/src/timeline/
  timeline-types.ts
  timeline-builder.ts
  timeline-reducer.ts
  timeline-selectors.ts
  scroll-controller.ts

packages/host-tui/src/renderer/opentui/timeline/
  TimelineView.tsx
  TimelineItemView.tsx
  UserMessageView.tsx
  AssistantMessageView.tsx
  ToolTimelineItem.tsx
  SummaryTimelineItem.tsx
  TimelineSeparator.tsx
  LatestIndicator.tsx
```

The renderer should expose `TimelineView` as the timeline entry point.

## Event inputs

Timeline consumes:

- session load/resume events
- user prompt submitted
- assistant text delta
- thinking delta
- tool call started
- tool call updated/completed
- assistant final message
- branch summary added
- compaction summary added
- stream aborted/error
- transcript replaced after resume/import

## Acceptance criteria

- Streaming assistant output updates one stable timeline item.
- Auto-scroll follows streaming only while user is at bottom.
- Manual scroll stops auto-follow.
- New output while manually scrolled increments pending indicator.
- Jump latest restores bottom anchor.
- Tool expansion state survives streaming updates.
- Timeline items have stable ids and type-specific rendering.
- Summary/tool/thinking/user/assistant items are visually distinct but not card-heavy.
- Timeline focus mode can scroll without interfering with editor input.
