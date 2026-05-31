/**
 * Session tree utilities — pure functions for tree building, labels, and search.
 */

import type { Message } from "piko-engine-protocol";
import type { SessionTreeEntry, SessionTreeNode } from "./session-types.js";

function extractTextContent(msg: Message): string {
  const content = msg.content;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .filter((c): c is { type: "text"; text: string } => c.type === "text")
      .map((c) => c.text)
      .join(" ");
  }
  return "";
}

/** Build a tree from flat entries using parentId links */
export function buildSessionTree(entries: SessionTreeEntry[]): SessionTreeNode[] {
  const byId = new Map<string, SessionTreeNode>();
  const roots: SessionTreeNode[] = [];

  for (const entry of entries) {
    byId.set(entry.id, { entry, children: [] });
  }

  for (const entry of entries) {
    const node = byId.get(entry.id)!;
    if (entry.parentId && byId.has(entry.parentId)) {
      byId.get(entry.parentId)!.children.push(node);
    } else {
      roots.push(node);
    }
  }

  // Resolve labels
  const labelStack: Array<{ name: string; parentId: string | null }> = [];
  for (const entry of entries) {
    if (entry.type === "session_info" && entry.name) {
      labelStack.push({ name: entry.name, parentId: entry.id });
    }
  }
  for (const { name, parentId } of labelStack) {
    if (parentId) {
      const node = byId.get(parentId);
      if (node) node.label = name;
    }
  }

  return roots;
}

/** Display label for a session entry (shown in tree views) */
export function getEntryLabel(entry: SessionTreeEntry): string {
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
        const tr = msg as { toolResult?: { name?: string } };
        return `[${tr.toolResult?.name ?? "tool"}]`;
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
  switch (entry.type) {
    case "message":
      return `${entry.message.role} ${extractTextContent(entry.message)}`;
    case "model_change":
      return `model ${entry.provider} ${entry.modelId}`;
    case "session_info":
      return `title ${entry.name ?? ""}`;
    case "branch_summary":
      return `branch ${entry.summary}`;
    case "compaction":
      return `compaction ${entry.summary}`;
    default:
      return entry.type;
  }
}
