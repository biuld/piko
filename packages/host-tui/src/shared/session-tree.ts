// ============================================================================
// Session tree builder — builds a tree from flat SessionTreeEntry[].
// Pure function operating on hostd-provided data.
// ============================================================================

import type { SessionTreeEntry, SessionTreeNode } from "./types.js";

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

  const labelsById = new Map<string, { label: string; timestamp: string }>();
  for (const entry of entries) {
    if (entry.type === "label") {
      if (entry.label) {
        labelsById.set(entry.targetId, { label: entry.label, timestamp: entry.timestamp });
      } else {
        labelsById.delete(entry.targetId);
      }
    }
  }
  for (const [targetId, labelInfo] of labelsById) {
    const node = byId.get(targetId);
    if (node) {
      node.label = labelInfo.label;
      node.labelTimestamp = labelInfo.timestamp;
    }
  }

  return roots;
}
