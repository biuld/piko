import type { SessionEntry } from "piko-host-runtime";
import { getTheme } from "../../theme.js";

interface ToolCallInfo {
  name: string;
  arguments: Record<string, unknown>;
}

export function hasTextContent(content: unknown): boolean {
  if (typeof content === "string") return content.trim().length > 0;
  if (Array.isArray(content)) {
    return content.some(
      (c) =>
        typeof c === "object" &&
        c !== null &&
        "type" in c &&
        c.type === "text" &&
        typeof (c as { text: string }).text === "string" &&
        (c as { text: string }).text.trim().length > 0,
    );
  }
  return false;
}

export function isSettingsEntry(entry: SessionEntry): boolean {
  return entry.type === "model_change" || entry.type === "session_info";
}

export function getEntryDisplayText(
  entry: SessionEntry,
  toolCallMap: Map<string, ToolCallInfo>,
): string {
  const t = getTheme();
  const normalize = (s: string) => s.replace(/[\n\t]/g, " ").trim();

  switch (entry.type) {
    case "message": {
      const msg = entry.message;
      if (msg.role === "user") {
        return (
          t.fg("accent", "user: ") + normalize(typeof msg.content === "string" ? msg.content : "")
        );
      }
      if (msg.role === "assistant") {
        const textContent = normalize(typeof msg.content === "string" ? msg.content : "");
        if (textContent) return t.fg("success", "assistant: ") + textContent;
        return t.fg("success", "assistant: ") + t.fg("muted", "(tool calls)");
      }
      const role = (msg as { role: string }).role;
      if (role === "toolResult") {
        const toolMsg = msg as { toolCallId?: string; toolName?: string };
        const toolCall = toolMsg.toolCallId ? toolCallMap.get(toolMsg.toolCallId) : undefined;
        if (toolCall) {
          const argsStr = JSON.stringify(toolCall.arguments);
          return t.fg("muted", `[${toolCall.name}] ${argsStr.slice(0, 80)}`);
        }
        return t.fg("muted", `[${toolMsg.toolName ?? "tool"}]`);
      }
      return t.fg("dim", `[${role}]`);
    }
    case "model_change":
      return t.fg("borderAccent", `model: ${entry.modelId}`);
    case "session_info":
      return entry.name ? t.fg("dim", `title: ${entry.name}`) : t.fg("dim", "title: (cleared)");
    default:
      return "";
  }
}
