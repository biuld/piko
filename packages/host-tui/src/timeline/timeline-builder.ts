// ============================================================================
// Timeline builder — converts domain transcript messages into timeline items
//
// Stable ID policy:
//   - User/assistant/system messages:  `msg:${messageId}`
//   - Tool calls:                      `tool:${toolCallId}`
// ============================================================================

import type { TuiMessageViewModel } from "../state/state.js";
import type { TimelineItem } from "./types.js";

/**
 * Build a single timeline item from a domain message view model.
 * The item id is derived from the message id (or toolCallId for tools),
 * ensuring stability across streaming → finalization and render cycles.
 */
export function buildTimelineItem(msg: TuiMessageViewModel): TimelineItem {
  const common = {
    messageId: msg.id,
    data: msg,
  };

  switch (msg.role) {
    case "user":
      return {
        id: `msg:${msg.id}`,
        kind: "user-message" as const,
        role: "user" as const,
        text: msg.text,
        createdAt: Date.now(),
        ...common,
      };

    case "assistant":
      return {
        id: `msg:${msg.id}`,
        kind: (msg.isStreaming ? "assistant-stream" : "assistant-message") as
          | "assistant-stream"
          | "assistant-message",
        role: "assistant" as const,
        text: msg.text,
        thinkingText: msg.thinkingText,
        isStreaming: msg.isStreaming,
        createdAt: Date.now(),
        ...common,
      };

    case "tool": {
      const tb = msg.toolBlock;
      const toolCallId = tb?.toolCallId ?? msg.id;
      const hasStructuredToolBlock = tb !== undefined;
      return {
        id: `tool:${toolCallId}`,
        kind: (tb?.status === "success" ||
        tb?.status === "error" ||
        (!hasStructuredToolBlock && msg.text)
          ? "tool-result"
          : "tool-call") as "tool-result" | "tool-call",
        role: "tool" as const,
        text: msg.text,
        toolCallId,
        toolName: tb?.name ?? "tool",
        toolStatus: tb?.status ?? (!hasStructuredToolBlock && msg.text ? "success" : "running"),
        toolArgs: tb?.args ?? {},
        toolResult: tb?.result ?? (!hasStructuredToolBlock && msg.text ? msg.text : undefined),
        toolDuration: tb?.duration,
        toolExitCode: tb?.exitCode,
        isCollapsed: tb?.isCollapsed ?? false,
        createdAt: Date.now(),
        ...common,
      };
    }

    case "branchSummary":
      return {
        id: `msg:${msg.id}`,
        kind: "branch-summary" as const,
        role: "system" as const,
        text: msg.text,
        createdAt: Date.now(),
        ...common,
      };

    case "compactionSummary":
      return {
        id: `msg:${msg.id}`,
        kind: "compaction-summary" as const,
        role: "system" as const,
        text: msg.text,
        tokensBefore: msg.tokensBefore,
        createdAt: Date.now(),
        ...common,
      };

    case "custom":
      return {
        id: `msg:${msg.id}`,
        kind: "system-note" as const,
        role: "system" as const,
        text: msg.text,
        customType: msg.customType,
        createdAt: Date.now(),
        ...common,
      };

    default:
      return {
        id: `msg:${msg.id}`,
        kind: "system-note" as const,
        role: "system" as const,
        text: msg.text,
        createdAt: Date.now(),
        ...common,
      };
  }
}

/**
 * Build timeline items from an array of messages, filtering out null entries
 * (metadata messages that update UI state rather than appearing in timeline).
 */
export function initTimelineItems(messages: TuiMessageViewModel[]): TimelineItem[] {
  return messages.map(buildTimelineItem);
}

/**
 * Create a streaming assistant timeline item for a new turn.
 */
export function createStreamingTimelineItem(
  messageId: string,
  text: string,
  thinkingText?: string,
): TimelineItem {
  return {
    id: `msg:${messageId}`,
    kind: "assistant-stream",
    role: "assistant",
    text,
    thinkingText,
    messageId,
    isStreaming: true,
    createdAt: Date.now(),
  };
}

/**
 * Update an existing streaming timeline item with new text.
 */
export function updateStreamingTimelineItem(
  items: TimelineItem[],
  itemId: string,
  text: string,
  thinkingText?: string,
): TimelineItem[] {
  const idx = items.findIndex((i) => i.id === itemId);
  if (idx < 0) return items;
  const updated = [...items];
  updated[idx] = {
    ...updated[idx],
    text,
    thinkingText: thinkingText ?? updated[idx].thinkingText,
    isStreaming: true,
  };
  return updated;
}

/**
 * Finalize a streaming timeline item into a completed assistant message.
 */
export function finalizeStreamingTimelineItem(
  items: TimelineItem[],
  itemId: string,
): TimelineItem[] {
  const idx = items.findIndex((i) => i.id === itemId);
  if (idx < 0) return items;
  const updated = [...items];
  updated[idx] = {
    ...updated[idx],
    kind: "assistant-message" as const,
    isStreaming: false,
  };
  return updated;
}

// ============================================================================
// Approval timeline item — for tools that require user confirmation
// ============================================================================
