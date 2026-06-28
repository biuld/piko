// ============================================================================
// Session content utilities — text extraction and tool call formatting.
// Pure functions operating on hostd-provided SessionTreeEntry data.
// ============================================================================

import type { PersistableMessage, SessionTreeEntry } from "./types.js";

export interface ToolCallInfo {
  name: string;
  args: Record<string, unknown>;
}

export function extractTextContent(msg: PersistableMessage): string {
  if ("content" in msg && msg.content !== undefined) {
    const content = msg.content;
    if (typeof content === "string") return content;
    if (Array.isArray(content)) {
      return content
        .filter((c): c is { type: "text"; text: string } => c.type === "text")
        .map((c) => c.text)
        .join(" ");
    }
  }
  return "";
}

export function collectToolCalls(entries: SessionTreeEntry[]): Map<string, ToolCallInfo> {
  const toolCalls = new Map<string, ToolCallInfo>();
  for (const entry of entries) {
    if (entry.type !== "message" || entry.message.role !== "assistant") continue;
    const content = (entry.message as { content?: unknown }).content;
    if (!Array.isArray(content)) continue;
    for (const block of content) {
      if (typeof block !== "object" || block === null || !("type" in block)) continue;
      if (block.type !== "toolCall") continue;
      const toolCall = block as { id?: string; name?: string; arguments?: unknown; args?: unknown };
      if (!toolCall.id || !toolCall.name) continue;
      const args =
        typeof toolCall.arguments === "object" && toolCall.arguments !== null
          ? toolCall.arguments
          : toolCall.args;
      toolCalls.set(toolCall.id, {
        name: toolCall.name,
        args: typeof args === "object" && args !== null ? (args as Record<string, unknown>) : {},
      });
    }
  }
  return toolCalls;
}

export function getToolResultInfo(
  msg: PersistableMessage,
  toolCalls?: Map<string, ToolCallInfo>,
): { name: string; args: Record<string, unknown> } {
  const toolMsg = msg as {
    toolCallId?: string;
    toolName?: string;
    toolResult?: { name?: string };
  };
  const call = toolMsg.toolCallId ? toolCalls?.get(toolMsg.toolCallId) : undefined;
  return {
    name: call?.name ?? toolMsg.toolName ?? toolMsg.toolResult?.name ?? "tool",
    args: call?.args ?? {},
  };
}

function shortenPath(path: string): string {
  const home = process.env.HOME || process.env.USERPROFILE || "";
  if (home && path.startsWith(home)) return `~${path.slice(home.length)}`;
  return path;
}

export function formatToolCall(name: string, args: Record<string, unknown>): string {
  switch (name) {
    case "read": {
      const path = shortenPath(String(args.path || args.file_path || ""));
      if (!path) return `[${name}]`;
      const offset = args.offset as number | undefined;
      const limit = args.limit as number | undefined;
      if (offset !== undefined || limit !== undefined) {
        const start = offset ?? 1;
        const end = limit !== undefined ? start + limit - 1 : "";
        return `[read: ${path}:${start}${end ? `-${end}` : ""}]`;
      }
      return `[read: ${path}]`;
    }
    case "write":
    case "edit": {
      const path = shortenPath(String(args.path || args.file_path || ""));
      if (!path) return `[${name}]`;
      return `[${name}: ${path}]`;
    }
    case "bash":
    case "exec": {
      const rawCmd = String(args.command || args.cmd || "");
      const cmd = rawCmd
        .replace(/[\n\t]/g, " ")
        .trim()
        .slice(0, 50);
      if (!cmd) return `[${name}]`;
      return `[bash: ${cmd}${rawCmd.length > 50 ? "..." : ""}]`;
    }
    case "grep": {
      const pattern = String(args.pattern || "");
      const path = shortenPath(String(args.path || "."));
      return `[grep: /${pattern}/ in ${path}]`;
    }
    case "find": {
      const pattern = String(args.pattern || args.glob || "");
      const path = shortenPath(String(args.path || args.directory || "."));
      return `[find: ${pattern} in ${path}]`;
    }
    case "ls": {
      const path = shortenPath(String(args.path || args.directory || "."));
      return `[ls: ${path}]`;
    }
    default:
      return `[${name}]`;
  }
}
