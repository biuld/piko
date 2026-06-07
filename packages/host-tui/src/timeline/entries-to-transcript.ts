// ============================================================================
// Session entry → Timeline transcript converter
//
// Converts SessionTreeEntry[] (from piko-session) into TuiMessageViewModel[]
// preserving metadata entries (model_change, thinking_level_change, etc.)
// that buildSessionContext normally filters out.
// ============================================================================

import type { SessionTreeEntry } from "piko-session";
import type { TuiMessageViewModel } from "../state/state.js";

const _msgSeq = 0;

/**
 * Convert a SessionTreeEntry array to TuiMessageViewModel array.
 * Handles all entry types: messages, custom messages, branch summaries,
 * compactions, AND metadata entries (model/thinking/session changes).
 */
export function entriesToTranscript(entries: SessionTreeEntry[]): TuiMessageViewModel[] {
  const result: TuiMessageViewModel[] = [];
  const toolMessageIndexByCallId = new Map<string, number>();

  for (const entry of entries) {
    switch (entry.type) {
      case "message": {
        const msg = entry.message;
        const role = mapMessageRole(msg.role);
        if (!role) continue;
        if (role === "assistant") {
          const thinkingText = extractThinking(msg as any);
          const text = extractText(msg);
          if (text || thinkingText) {
            result.push({
              id: entry.id,
              role,
              text,
              thinkingText,
            });
          }

          for (const toolCall of extractToolCalls(msg as { content?: unknown })) {
            const toolMsg: TuiMessageViewModel = {
              id: `${entry.id}:${toolCall.id}`,
              role: "tool",
              text: "",
              toolBlock: {
                toolCallId: toolCall.id,
                name: toolCall.name,
                args: toolCall.args,
                status: "running",
                isCollapsed: false,
              },
            };
            toolMessageIndexByCallId.set(toolCall.id, result.length);
            result.push(toolMsg);
          }
        } else if (role === "tool") {
          const toolResult = getToolResult(msg);
          const existingIndex = toolMessageIndexByCallId.get(toolResult.toolCallId);
          if (existingIndex !== undefined) {
            const existing = result[existingIndex];
            result[existingIndex] = {
              ...existing,
              text: "",
              toolBlock: {
                ...existing.toolBlock!,
                status: toolResult.isError ? "error" : "success",
                result: toolResult.result,
              },
            };
          } else {
            result.push({
              id: entry.id,
              role,
              text: "",
              toolBlock: {
                toolCallId: toolResult.toolCallId,
                name: toolResult.toolName,
                args: {},
                status: toolResult.isError ? "error" : "success",
                result: toolResult.result,
                isCollapsed: false,
              },
            });
          }
        } else {
          result.push({
            id: entry.id,
            role,
            text: extractText(msg),
          });
        }
        break;
      }

      case "custom_message": {
        result.push({
          id: entry.id,
          role: "custom",
          customType: entry.customType,
          text:
            typeof entry.content === "string"
              ? entry.content
              : Array.isArray(entry.content)
                ? entry.content
                    .filter((c): c is { type: "text"; text: string } => c.type === "text")
                    .map((c) => c.text)
                    .join("\n")
                : "",
        });
        break;
      }

      case "branch_summary": {
        result.push({
          id: entry.id,
          role: "branchSummary",
          text: entry.summary,
        });
        break;
      }

      case "compaction": {
        result.push({
          id: entry.id,
          role: "compactionSummary",
          text: entry.summary,
        });
        break;
      }

      case "model_change":
      case "thinking_level_change":
      case "session_info":
        // These update UI state (status bar, thinking pill, header)
        // rather than appearing as timeline items. Skip them.
        break;

      // Skip these: not visible in timeline
      case "active_tools_change":
      case "custom":
      case "label":
      case "leaf":
        break;
    }
  }

  return result;
}

// ============================================================================
// Helpers
// ============================================================================

function mapMessageRole(role: string): TuiMessageViewModel["role"] | null {
  switch (role) {
    case "user":
      return "user";
    case "assistant":
      return "assistant";
    case "toolResult":
      return "tool";
    default:
      return null;
  }
}

function extractText(msg: { content?: unknown; role?: string }): string {
  if ("content" in msg && msg.content !== undefined) {
    const content = msg.content;
    if (typeof content === "string") return content;
    if (Array.isArray(content)) {
      return content
        .filter((c): c is { type: "text"; text: string } => (c as any).type === "text")
        .map((c: any) => c.text)
        .join("\n");
    }
  }
  return "";
}

function extractToolCalls(msg: { content?: unknown }): Array<{
  id: string;
  name: string;
  args: Record<string, unknown>;
}> {
  const content = msg.content;
  if (!Array.isArray(content)) return [];
  const toolCalls: Array<{ id: string; name: string; args: Record<string, unknown> }> = [];
  for (const block of content) {
    if (typeof block !== "object" || block === null || (block as any).type !== "toolCall") continue;
    const id = (block as any).id;
    const name = (block as any).name;
    if (typeof id !== "string" || typeof name !== "string") continue;
    const rawArgs = (block as any).arguments ?? (block as any).args;
    toolCalls.push({
      id,
      name,
      args: typeof rawArgs === "object" && rawArgs !== null ? rawArgs : {},
    });
  }
  return toolCalls;
}

function getToolResult(msg: { content?: unknown; role?: string }): {
  toolCallId: string;
  toolName: string;
  result: unknown;
  isError: boolean;
} {
  const toolMsg = msg as {
    toolCallId?: string;
    toolName?: string;
    details?: unknown;
    isError?: boolean;
    content?: unknown;
  };
  return {
    toolCallId: toolMsg.toolCallId ?? "tool",
    toolName: toolMsg.toolName ?? "tool",
    result: toolMsg.details ?? extractText(toolMsg),
    isError: toolMsg.isError === true,
  };
}

function extractThinking(msg: { content?: unknown; role?: string }): string | undefined {
  if ("content" in msg && msg.content !== undefined) {
    const content = msg.content;
    if (Array.isArray(content)) {
      const thinking = content
        .filter((c): c is { type: "thinking"; thinking: string } => (c as any).type === "thinking")
        .map((c: any) => c.thinking)
        .join("\n");
      return thinking || undefined;
    }
  }
  return undefined;
}
