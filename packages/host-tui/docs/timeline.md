# Timeline System

Timeline turns Host/Orchestrator events and session transcript data into a
scrollable, interactive message timeline with deterministic ordering.

## Architecture

The timeline system has two layers:

1. **TimelineProjection** (`src/timeline/projection.ts`) — deterministic,
   ID-keyed item ordering. Items are ordered by session-transcript position,
   not event arrival time.
2. **TuiTimelineState** (`src/timeline/types.ts`) — scroll anchor, expansion
   state, streaming metadata.

## TimelineProjection

```ts
interface TimelineProjection {
  orderedIds: string[];               // ordered item IDs (msg:<id> or tool:<callId>)
  itemsById: Record<string, TimelineItem>;  // all items keyed by stable ID
  lastAppliedSeqByRun: Record<string, number>; // sequence validation per run
  pendingTools: Record<string, TimelineItem[]>;  // tools waiting for parent message
}
```

### Ordering rules

- **Messages**: Appended at end of `orderedIds` (append-only during live
  streaming). Inserted once on `message_start`, updated on `message_update`
  and `message_end`.
- **Tools**: Inserted immediately after their parent assistant message,
  ordered by `toolCallIndex` within that parent. If the parent hasn't arrived
  yet, tools are stored in `pendingTools` and re-parented when the parent
  arrives.
- **Legacy sessions**: `buildOrderedProjection()` preserves original transcript
  adjacency (tools stay near parent assistant messages).

### Pure reducer functions

The projection is manipulated through pure functions:

```ts
upsertUserMessage(proj, item) → TimelineProjection
upsertAssistantMessage(proj, item) → TimelineProjection
upsertToolItem(proj, item, parentMessageId, toolCallIndex) → TimelineProjection
validateAndApplySeq(proj, runId, eventSeq) → { proj, diagnostics }
buildOrderedProjection(items) → TimelineProjection
```

Sequence validation via `validateAndApplySeq()` tracks `lastAppliedSeqByRun`
and produces `ProjectionDiagnostic` entries for regressions.

## Timeline items

```ts
type TimelineItemKind =
  | "user-message" | "assistant-message" | "assistant-stream"
  | "tool-call" | "tool-result"
  | "branch-summary" | "compaction-summary"
  | "system-note" | "approval" | "notification-ref";

interface TimelineItem {
  id: string;
  kind: TimelineItemKind;
  role?: "user" | "assistant" | "tool" | "system";
  text?: string;
  createdAt?: number;
  messageId?: string;
  toolCallId?: string;
  toolName?: string;
  toolStatus?: "pending" | "running" | "success" | "error";
  toolArgs?: unknown;
  toolResult?: unknown;
  toolDuration?: number;
  toolExitCode?: number;
  customType?: string;
  parentId?: string;
  isStreaming?: boolean;
  isCollapsed?: boolean;
  severity?: "info" | "success" | "warning" | "error";
  thinkingText?: string;
  hideThinking?: boolean;
  isError?: boolean;
  errorMessage?: string;
  tokensBefore?: number;
  message?: RuntimeMessage;             // full structured message
  content?: RuntimeAssistantContentBlock[]; // ordered content blocks
  // Ordering metadata
  messageIndex?: number;
  turnIndex?: number;
  eventSeq?: number;
  parentMessageId?: string;
  contentIndex?: number;
  toolCallIndex?: number;
}
```

## Scroll state

```ts
type TimelineAnchor = "bottom" | "manual" | "item";

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

- `"bottom"`: auto-follow new streaming output and new messages.
- `"manual"`: user has scrolled; do not force scroll. Show pending indicator.
- `"item"`: keep a specific item visible (selection/search result).

## Streaming behavior

1. User submits prompt → user message item appended to projection.
2. `message_start` → assistant stream item created with stable `msg:<id>`.
3. `message_update` → text deltas update the same item in `itemsById`.
4. Tool call start/end → tool items inserted after parent via `upsertToolItem`.
5. `message_end` → final assistant item, stream state returns to idle.

Requirements:

- Do not create a new assistant item for every text delta.
- Preserve scroll position when user is in manual mode.
- If anchor is `"bottom"`, scroll to bottom after each update.
- If anchor is `"manual"`, increment `pendingNewItems` instead.
- Show latest/new-output indicator when pending items exist.

## Scroll behavior

- Start with `anchor = "bottom"`, `atBottom = true`.
- Auto-scroll during streaming while `atBottom === true`.
- User scroll up → `anchor = "manual"`, `userScrolled = true`.
- New items while manual → increment `pendingNewItems`.
- User returns to bottom → `anchor = "bottom"`, clear pending count.
- Jump latest keybinding restores `"bottom"` anchor.

Scroll commands are dispatched via `state.scrollCommand`:
```ts
scrollCommand?: { dir: "pageUp" | "pageDown" | "jumpLatest"; seq: number } | null;
```

The editor timeline-scroll interceptor dispatches PageUp/PageDown/End via
scroll commands with incrementing sequence numbers for change detection.

## Rendering policy by item type

- **User message**: Labeled/visually distinct, multi-line content preserved.
- **Assistant message**: Plain readable text. Streaming state shows subtle
  cursor.
- **Assistant stream**: Same visual as assistant. Stable item while streaming.
  Finalizes without visual jump.
- **Tool call/result**: Tool name, status, summary. Collapsed by default when
  long. Expansion stored in `collapsedToolCallIds`.
- **Thinking**: Subdued expandable content, respects `hideThinking` setting.
- **Branch/compaction summaries**: Restrained accent, visually separated from
  normal messages.
- **Approval**: Inline approval result after completion.
- **System note**: Session lifecycle notes only when they belong in transcript
  context.

## Integration

Timeline consumes events via the TUI event stream:

- `message_start` / `message_update` / `message_end` → projection upserts
- `tool_call_started` / `tool_call_ended` → tool upserts
- `turn_finished` → final transcript rebuild
- `session_resumed` → full rebuild via `buildOrderedProjection()`
- `chat_scrolled` → scroll state update
- `timeline_jump_latest` → reset anchor to `"bottom"`
- `timeline_toggle_all_tools` → collapse/expand all tools

Timeline scroll is managed by `ScrollController` (`src/timeline/scroll-controller.ts`).

## Transcription

`entriesToTranscript()` (in `src/timeline/entries-to-transcript.ts`) converts
raw session entries into `TuiMessageViewModel[]`. This is a TUI-layer
projection and does not live in Host.

## Layout

Timeline receives a row budget from `computeRegionHeights()`:
```ts
interface TimelineLayout {
  width: number;
  height: number;
  mode: "regular" | "compact" | "minimal";
}
```

Timeline height = remaining area after status, partial panel, editor, bottom bar.
