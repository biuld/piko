// ============================================================================
// Timeline types — timeline item model, scroll state
// ============================================================================

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
  /** Preserved customType from custom_message entries */
  customType?: string;
  parentId?: string;
  isStreaming?: boolean;
  isCollapsed?: boolean;
  severity?: "info" | "success" | "warning" | "error";
  data?: unknown;
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
