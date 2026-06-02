// ============================================================================
// Timeline builder — converts domain transcript events into timeline items
// ============================================================================

import type { TuiMessageViewModel } from "../state/state.js";
import type { TimelineItem } from "./types.js";

let timelineItemCounter = 0;

function nextId(): string {
  return `tl-${++timelineItemCounter}`;
}

/**
 * Build timeline items from view model messages.
 */
export function buildTimelineItems(messages: TuiMessageViewModel[]): TimelineItem[] {
  const items: TimelineItem[] = [];

  for (const msg of messages) {
    switch (msg.role) {
      case "user":
        items.push({
          id: nextId(),
          kind: "user-message",
          role: "user",
          text: msg.text,
          messageId: msg.id,
          data: msg,
        });
        break;

      case "assistant":
        items.push({
          id: nextId(),
          kind: msg.isStreaming ? "assistant-stream" : "assistant-message",
          role: "assistant",
          text: msg.text,
          messageId: msg.id,
          isStreaming: msg.isStreaming,
          data: msg,
        });
        break;

      case "tool":
        items.push({
          id: nextId(),
          kind: msg.toolBlock?.status === "success" ? "tool-result" : "tool-call",
          role: "tool",
          text: msg.text,
          messageId: msg.id,
          toolCallId: msg.toolBlock?.toolCallId,
          toolName: msg.toolBlock?.name,
          toolStatus: msg.toolBlock?.status,
          toolArgs: msg.toolBlock?.args,
          toolResult: msg.toolBlock?.result,
          isCollapsed: msg.toolBlock?.isCollapsed ?? false,
          data: msg,
        });
        break;

      case "branchSummary":
        items.push({
          id: nextId(),
          kind: "branch-summary",
          role: "system",
          text: msg.text,
          messageId: msg.id,
          data: msg,
        });
        break;

      case "compactionSummary":
        items.push({
          id: nextId(),
          kind: "compaction-summary",
          role: "system",
          text: msg.text,
          messageId: msg.id,
          data: msg,
        });
        break;

      default:
        // Unknown roles become system notes
        items.push({
          id: nextId(),
          kind: "system-note",
          role: "system",
          text: msg.text,
          messageId: msg.id,
          data: msg,
        });
        break;
    }
  }

  return items;
}

/**
 * Create or update the streaming timeline item during streaming.
 */
export function updateStreamingItem(
  items: TimelineItem[],
  text: string,
  streamingItemId: string | undefined,
): { items: TimelineItem[]; streamingItemId: string } {
  if (streamingItemId) {
    // Update existing streaming item
    const idx = items.findIndex((i) => i.id === streamingItemId);
    if (idx >= 0) {
      const updated = [...items];
      updated[idx] = { ...updated[idx], text, isStreaming: true };
      return { items: updated, streamingItemId };
    }
  }

  // Create new streaming item
  const id = nextId();
  const newItem: TimelineItem = {
    id,
    kind: "assistant-stream",
    role: "assistant",
    text,
    isStreaming: true,
  };
  return { items: [...items, newItem], streamingItemId: id };
}

/**
 * Finalize a streaming item into a completed assistant message.
 */
export function finalizeStreamingItem(
  items: TimelineItem[],
  streamingItemId: string,
): TimelineItem[] {
  const idx = items.findIndex((i) => i.id === streamingItemId);
  if (idx < 0) return items;

  const updated = [...items];
  updated[idx] = {
    ...updated[idx],
    kind: "assistant-message",
    isStreaming: false,
  };
  return updated;
}
