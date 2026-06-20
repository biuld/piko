// ============================================================================
import type { RuntimeAssistantContentBlock, RuntimeMessage } from "piko-orchestrator-protocol";

export type TimelineItemKind =
  // Messages
  | "user-message"
  | "assistant-message"
  | "assistant-stream"
  // Tools
  | "tool-call"
  | "tool-result"
  // Structural
  | "branch-summary"
  | "compaction-summary"
  // Special
  | "system-note"
  | "approval"
  | "notification-ref";

export type TimelineAnchor = "bottom" | "manual" | "item";

export interface TimelineItem {
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
  /** Duration of tool execution in milliseconds */
  toolDuration?: number;
  /** Exit code for bash/exec tools */
  toolExitCode?: number;
  /** Preserved customType from custom_message entries */
  customType?: string;
  parentId?: string;
  isStreaming?: boolean;
  isCollapsed?: boolean;
  severity?: "info" | "success" | "warning" | "error";
  data?: unknown;

  // ---- Thinking ----
  /** Thinking text to render inline in assistant message */
  thinkingText?: string;
  /** Whether thinking blocks are hidden (shows "Thinking..." label) */
  hideThinking?: boolean;

  // ---- Error ----
  /** Whether this message represents an error state */
  isError?: boolean;
  /** Error message to display */
  errorMessage?: string;

  // ---- Summary ----
  /** Token count before compaction (for compaction summaries) */
  tokensBefore?: number;
  /** Structured RuntimeMessage payload for block-based rendering */
  message?: RuntimeMessage;
  /** Ordered assistant content blocks */
  content?: RuntimeAssistantContentBlock[];

  // ---- Ordering metadata (from protocol) -------
  /** Stable messageIndex from the orchestrator protocol. */
  messageIndex?: number;
  /** Turn index (zero-based step count). */
  turnIndex?: number;
  /** Event sequence for monotonicity validation. */
  eventSeq?: number;
  /** Parent message ID for tool positioning. */
  parentMessageId?: string;
  /** Content block index in the parent message. */
  contentIndex?: number;
  /** Dense tool call index (0, 1, 2...) within the parent message. */
  toolCallIndex?: number;
}

export interface TuiTimelineState {
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

export interface TimelineLayout {
  width: number;
  height: number;
  mode: "regular" | "compact" | "minimal";
}

export function createDefaultTimelineState(): TuiTimelineState {
  return {
    items: [],
    anchor: "bottom",
    atBottom: true,
    userScrolled: false,
    pendingNewItems: 0,
    expandedItemIds: new Set(),
    collapsedToolCallIds: new Set(),
  };
}
