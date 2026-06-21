import type { SessionTreeEntry, SessionTreeNode } from "../session-types.js";
import {
  extractTextContent,
  formatToolCall,
  getToolResultInfo,
  type ToolCallInfo,
} from "./content.js";

/** Display label for a session entry (shown in tree views) */
export function getEntryLabel(
  entry: SessionTreeEntry,
  toolCalls?: Map<string, ToolCallInfo>,
): string {
  switch (entry.type) {
    case "message": {
      const msg = entry.message;
      if (msg.role === "user") {
        return `user: ${extractTextContent(msg).slice(0, 120)}`;
      }
      if (msg.role === "assistant") {
        const text = extractTextContent(msg);
        if (text) return `assistant: ${text.slice(0, 120)}`;
        return "assistant: (tool calls)";
      }
      if (msg.role === "toolResult") {
        const tool = getToolResultInfo(msg, toolCalls);
        return formatToolCall(tool.name, tool.args);
      }
      return "[message]";
    }
    case "model_change":
      return `model: ${entry.provider}/${entry.modelId}`;
    case "session_info":
      return entry.name ? `title: ${entry.name}` : "title: (cleared)";
    case "branch_summary":
      return `branch: ${entry.summary.slice(0, 80)}`;
    case "compaction":
      return `compaction: ${entry.summary.slice(0, 80)}`;
    default:
      return entry.type;
  }
}

/** Searchable text for a tree node (used by tree selector filtering) */
export function getSearchableText(node: SessionTreeNode): string {
  const entry = node.entry;
  const parts: string[] = [];
  if (node.label) parts.push(node.label);
  switch (entry.type) {
    case "message":
      parts.push(entry.message.role, extractTextContent(entry.message));
      break;
    case "custom_message":
      parts.push(
        entry.customType,
        typeof entry.content === "string"
          ? entry.content
          : entry.content
              .filter((c): c is { type: "text"; text: string } => c.type === "text")
              .map((c) => c.text)
              .join(" "),
      );
      break;
    case "model_change":
      parts.push("model", entry.provider, entry.modelId);
      break;
    case "thinking_level_change":
      parts.push("thinking", entry.thinkingLevel);
      break;
    case "session_info":
      parts.push("title", entry.name ?? "");
      break;
    case "branch_summary":
      parts.push("branch summary", entry.summary);
      break;
    case "compaction":
      parts.push("compaction", entry.summary);
      break;
    case "custom":
      parts.push("custom", entry.customType);
      break;
    case "label":
      parts.push("label", entry.label ?? "");
      break;
    default:
      parts.push(entry.type);
  }
  return parts.join(" ");
}

/** A colored text segment for use in SelectListView / rich text rendering. */
export interface TextSegment {
  text: string;
  /** Theme token path, e.g. "text.accent", "text.muted", "border.accent" */
  color?: string;
}

/**
 * Get colored text segments for a session tree entry.
 * Mirrors pi's getEntryDisplayText color scheme.
 */
export function getEntrySegments(
  entry: SessionTreeEntry,
  toolCalls?: Map<string, ToolCallInfo>,
): TextSegment[] {
  switch (entry.type) {
    case "message": {
      const msg = entry.message;
      if (msg.role === "user") {
        return [
          { text: "user: ", color: "text.accent" },
          { text: extractTextContent(msg).slice(0, 200) },
        ];
      }
      if (msg.role === "assistant") {
        const text = extractTextContent(msg);
        if (text) {
          return [{ text: "assistant: ", color: "text.success" }, { text: text.slice(0, 200) }];
        }
        return [{ text: "assistant: (tool calls)", color: "text.success" }];
      }
      if (msg.role === "toolResult") {
        const tool = getToolResultInfo(msg, toolCalls);
        return [{ text: formatToolCall(tool.name, tool.args), color: "text.muted" }];
      }
      return [{ text: `[${msg.role}]`, color: "text.dim" }];
    }
    case "model_change":
      return [{ text: `model: ${entry.provider}/${entry.modelId}`, color: "text.dim" }];
    case "thinking_level_change":
      return [{ text: `thinking: ${entry.thinkingLevel}`, color: "text.dim" }];
    case "session_info":
      return [
        {
          text: entry.name ? `title: ${entry.name}` : "title: (cleared)",
          color: "text.dim",
        },
      ];
    case "branch_summary":
      return [{ text: "branch: ", color: "text.warning" }, { text: entry.summary.slice(0, 200) }];
    case "compaction":
      return [
        { text: "compaction: ", color: "border.accent" },
        { text: entry.summary.slice(0, 200) },
      ];
    case "label":
      return [{ text: `label: ${entry.label ?? "(cleared)"}`, color: "text.dim" }];
    default:
      return [{ text: entry.type, color: "text.dim" }];
  }
}
