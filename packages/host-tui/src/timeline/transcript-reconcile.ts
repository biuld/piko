// ============================================================================
// Transcript reconciliation — pure function that merges the canonical engine
// transcript with the streaming-incremental transcript, preserving stable
// message IDs and avoiding unnecessary timeline item rebuilds.
// ============================================================================

import type { Message } from "piko-engine-protocol";
import type { TuiMessageViewModel } from "../state/state.js";
import { buildTimelineItem } from "./timeline-builder.js";
import type { TimelineItem } from "./types.js";

export interface ReconcileResult {
  transcript: TuiMessageViewModel[];
  timelineItems: TimelineItem[];
}

export interface ReconcileOptions {
  /** Allocator for new message IDs. Injected to keep this function pure. */
  createMessageId: () => string;
}

/**
 * Reconcile canonical engine messages with the streaming-incremental transcript.
 * Returns new transcript array and timeline items, reusing existing IDs where possible.
 * Pure function — all side effects (ID allocation) are injected via options.
 */
export function reconcileTranscript(
  canonicalMessages: Message[],
  existingTranscript: TuiMessageViewModel[],
  existingTimelineItems: TimelineItem[],
  options: ReconcileOptions,
): ReconcileResult {
  const reconciled: TuiMessageViewModel[] = [];
  const usedExistingIds = new Set<string>();
  const emittedToolCallIds = new Set<string>();

  const takeExisting = (
    predicate: (msg: TuiMessageViewModel) => boolean,
  ): TuiMessageViewModel | undefined => {
    const match = existingTranscript.find((msg) => !usedExistingIds.has(msg.id) && predicate(msg));
    if (match) usedExistingIds.add(match.id);
    return match;
  };

  const textFromContent = (content: unknown): string => {
    if (typeof content === "string") return content;
    if (Array.isArray(content)) {
      return content
        .filter((block: any) => block.type === "text")
        .map((block: any) => block.text)
        .join("\n");
    }
    return "";
  };

  const upsertTool = (toolMsg: TuiMessageViewModel): void => {
    const toolCallId = toolMsg.toolBlock?.toolCallId;
    if (!toolCallId) {
      reconciled.push(toolMsg);
      return;
    }
    const existingIdx = reconciled.findIndex((msg) => msg.toolBlock?.toolCallId === toolCallId);
    if (existingIdx >= 0) {
      reconciled[existingIdx] = {
        ...reconciled[existingIdx],
        toolBlock: {
          ...reconciled[existingIdx].toolBlock!,
          ...toolMsg.toolBlock!,
        },
      };
      emittedToolCallIds.add(toolCallId);
      return;
    }
    reconciled.push(toolMsg);
    emittedToolCallIds.add(toolCallId);
  };

  for (const msg of canonicalMessages) {
    if (msg.role === "user") {
      const text = textFromContent((msg as any).content);
      const existingUser =
        takeExisting((m) => m.role === "user" && m.text === text) ??
        takeExisting((m) => m.role === "user");
      if (existingUser) {
        reconciled.push({ ...existingUser, text: text || existingUser.text });
      } else {
        reconciled.push({ id: options.createMessageId(), role: "user", text });
      }
    } else if (msg.role === "assistant") {
      const content = (msg as any).content;
      const text = textFromContent(content);
      const existingAsst =
        takeExisting((m) => m.role === "assistant" && m.isStreaming === true) ??
        takeExisting((m) => m.role === "assistant" && m.text === text) ??
        takeExisting((m) => m.role === "assistant");

      if (Array.isArray(content)) {
        if (existingAsst) {
          reconciled.push({
            ...existingAsst,
            text: text || existingAsst.text,
            isStreaming: false,
          });
        } else if (text) {
          reconciled.push({ id: options.createMessageId(), role: "assistant", text });
        }

        for (const block of content) {
          if (block.type === "toolCall") {
            const toolCallId = block.id ?? `tc-${reconciled.length}`;
            if (emittedToolCallIds.has(toolCallId)) continue;
            const existingTool = existingTranscript.find(
              (m) => m.toolBlock?.toolCallId === toolCallId,
            );
            if (existingTool) {
              usedExistingIds.add(existingTool.id);
              upsertTool(existingTool);
            } else {
              upsertTool({
                id: options.createMessageId(),
                role: "tool",
                text: "",
                toolBlock: {
                  toolCallId,
                  name: block.name ?? "unknown",
                  args: block.arguments ?? block.args ?? {},
                  status: "success" as const,
                  isCollapsed: false,
                },
              });
            }
          }
        }
      } else if (existingAsst) {
        reconciled.push({
          ...existingAsst,
          text: text || existingAsst.text,
          isStreaming: false,
        });
      } else if (text) {
        reconciled.push({ id: options.createMessageId(), role: "assistant", text });
      }
    } else if (msg.role === "toolResult" || (msg as any).role === "tool") {
      const anyMsg = msg as any;
      const toolCallId = anyMsg.toolCallId ?? `tc-${reconciled.length}`;
      const existingTool = existingTranscript.find((m) => m.toolBlock?.toolCallId === toolCallId);
      if (existingTool) {
        usedExistingIds.add(existingTool.id);
        upsertTool({
          ...existingTool,
          toolBlock: {
            ...existingTool.toolBlock!,
            status: (anyMsg.isError ? "error" : "success") as "error" | "success",
            result: anyMsg.content ?? anyMsg.details,
          },
        });
      } else {
        upsertTool({
          id: options.createMessageId(),
          role: "tool",
          text: "",
          toolBlock: {
            toolCallId,
            name: anyMsg.toolName ?? "tool",
            args: {},
            status: (anyMsg.isError ? "error" : "success") as "error" | "success",
            result: anyMsg.content ?? anyMsg.details,
            isCollapsed: false,
          },
        });
      }
    }
  }

  const timelineItems: TimelineItem[] = reconciled.map((msg) => {
    const itemId = msg.toolBlock?.toolCallId ? `tool:${msg.toolBlock.toolCallId}` : `msg:${msg.id}`;
    const existingItem = existingTimelineItems.find((i) => i.id === itemId);
    if (existingItem) {
      if (msg.role === "assistant") {
        return {
          ...existingItem,
          text: msg.text,
          isStreaming: false,
          kind: "assistant-message" as const,
        };
      }
      if (msg.role === "tool" && msg.toolBlock) {
        return {
          ...existingItem,
          toolStatus: msg.toolBlock.status,
          toolResult: msg.toolBlock.result,
          kind: "tool-result" as const,
        };
      }
      return existingItem;
    }
    return buildTimelineItem(msg);
  });

  return {
    transcript: reconciled.length > 0 ? reconciled : existingTranscript,
    timelineItems: reconciled.length > 0 ? timelineItems : existingTimelineItems,
  };
}
