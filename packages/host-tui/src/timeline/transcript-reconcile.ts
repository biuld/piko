// ============================================================================
// Transcript reconciliation — pure functions for live finalization
// and legacy session loading.
//
// Three operations:
//   1. finalizeProjection   — validate + complete live projection with canonical content
//   2. reconcileLegacyTranscript — compatibility path for sessions without runtime IDs
//   3. validateCommittedTranscript — diagnostic check only
// ============================================================================

import type { Message } from "../shared/index.js";
import type { TuiMessageViewModel } from "../state/state.js";
import type { ProjectionDiagnostic, TimelineProjection } from "./projection.js";
import { buildTimelineItem } from "./timeline-builder.js";
import type { TimelineItem } from "./types.js";

// ---- Live finalization -------

export interface FinalizeOptions {
  /** Allocator for new message IDs for genuinely missing entities. */
  createMessageId: () => string;
}

export interface FinalizeResult {
  projection: TimelineProjection;
  diagnostics: ProjectionDiagnostic[];
}

/**
 * Finalize a live projection against the canonical transcript.
 *
 * Live message content is authoritative at `message_end`; the commit phase
 * therefore never guesses message identity from array position. Tool results
 * can be refreshed because toolCallId is stable end-to-end. The durable
 * SessionTreeEntry snapshot is reconciled separately by entry ID.
 */
export function finalizeProjection(
  proj: TimelineProjection,
  canonicalMessages: Message[],
): FinalizeResult {
  const diagnostics: ProjectionDiagnostic[] = [];
  const itemsById = { ...proj.itemsById };
  const orderedIds = [...proj.orderedIds];

  // Canonical tool calls and results are separate pi messages, while the
  // timeline projects them as one item. Stable toolCallId is the only join key.
  const toolResultsById = new Map<
    string,
    { toolName: string; isError: boolean; result: unknown }
  >();
  for (const msg of canonicalMessages) {
    switch (msg.role) {
      case "user":
      case "assistant":
        break;
      case "toolResult":
        toolResultsById.set(msg.toolCallId, {
          toolName: msg.toolName,
          isError: msg.isError,
          result: msg.details ?? msg.content,
        });
        break;
      default:
        assertNever(msg);
    }
  }

  for (const id of orderedIds) {
    const item = itemsById[id];
    if (!item) continue;

    if (id.startsWith("msg:")) {
      itemsById[id] = {
        ...item,
        kind: item.kind === "assistant-stream" ? "assistant-message" : item.kind,
        isStreaming: false,
      };
    } else if (id.startsWith("tool:")) {
      const result = item.toolCallId ? toolResultsById.get(item.toolCallId) : undefined;
      if (!result) continue;
      itemsById[id] = {
        ...item,
        kind: "tool-result",
        toolName: result.toolName || item.toolName,
        toolStatus: result.isError ? "error" : "success",
        toolResult: result.result,
        text:
          typeof result.result === "string"
            ? result.result
            : result.result === undefined
              ? ""
              : JSON.stringify(result.result),
      };
    }
  }

  return {
    projection: { ...proj, itemsById, orderedIds },
    diagnostics,
  };
}

function assertNever(value: never): never {
  throw new Error(`Unsupported canonical message: ${JSON.stringify(value)}`);
}

// ---- Diagnostic validation -------

export function validateCommittedTranscript(
  proj: TimelineProjection,
  messages: Message[],
): ProjectionDiagnostic[] {
  const diagnostics: ProjectionDiagnostic[] = [];

  for (const msg of messages) {
    if (msg.role === "assistant") {
      const content = (msg as any).content;
      if (Array.isArray(content)) {
        for (const block of content) {
          if (block.type === "toolCall" && block.id) {
            const hasTool = Object.values(proj.itemsById).some(
              (item) => item.toolCallId === block.id,
            );
            if (!hasTool) {
              const toolId = `tool:${block.id}`;
              diagnostics.push({ kind: "missing_parent", toolId, parentMessageId: "" });
            }
          }
        }
      }
    }
  }

  return diagnostics;
}

// ---- Legacy reconciliation -------

export interface ReconcileOptions {
  /** Allocator for new message IDs. Injected to keep this function pure. */
  createMessageId: () => string;
}

/**
 * Reconcile canonical engine messages with the streaming-incremental transcript.
 * This is the LEGACY path for sessions loaded without runtime ordering IDs.
 * Returns new transcript array and timeline items.
 */
export function reconcileLegacyTranscript(
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

  const thinkingFromContent = (content: unknown): string | undefined => {
    if (Array.isArray(content)) {
      const thinking = (content as any[])
        .filter((block: any) => block.type === "thinking")
        .map((block: any) => block.thinking)
        .join("\n");
      return thinking || undefined;
    }
    return undefined;
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
      const thinkingText = thinkingFromContent(content);

      if (Array.isArray(content)) {
        if (text || thinkingText) {
          const existingAsst =
            takeExisting((m) => m.role === "assistant" && m.isStreaming === true) ??
            takeExisting((m) => m.role === "assistant" && m.text === text) ??
            takeExisting((m) => m.role === "assistant");

          if (existingAsst) {
            reconciled.push({
              ...existingAsst,
              text: text || existingAsst.text,
              thinkingText: thinkingText ?? existingAsst.thinkingText,
              isStreaming: false,
            });
          } else {
            reconciled.push({
              id: options.createMessageId(),
              role: "assistant",
              text,
              thinkingText,
            });
          }
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
      } else if (text || thinkingText) {
        const existingAsst =
          takeExisting((m) => m.role === "assistant" && m.isStreaming === true) ??
          takeExisting((m) => m.role === "assistant");
        if (existingAsst) {
          reconciled.push({
            ...existingAsst,
            text: text || existingAsst.text,
            thinkingText: thinkingText ?? existingAsst.thinkingText,
            isStreaming: false,
          });
        } else {
          reconciled.push({
            id: options.createMessageId(),
            role: "assistant",
            text,
            thinkingText,
          });
        }
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
          thinkingText: msg.thinkingText,
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

export interface ReconcileResult {
  transcript: TuiMessageViewModel[];
  timelineItems: TimelineItem[];
}
