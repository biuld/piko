/**
 * Session tree utilities — pure functions for tree building, labels, and search.
 */

import type { AgentMessage } from "piko-session";
import type { SessionTreeEntry, SessionTreeNode } from "./session-types.js";

function extractTextContent(msg: AgentMessage): string {
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

  // Resolve labels: session_info entries label their parent
  const labelStack: Array<{ name: string; parentId: string | null }> = [];
  for (const entry of entries) {
    if (entry.type === "session_info" && entry.name) {
      labelStack.push({ name: entry.name, parentId: entry.parentId });
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

// ============================================================================
// Rich text segments — pi-style colored entry rendering
// ============================================================================

/** A colored text segment for use in SelectListView / rich text rendering. */
export interface TextSegment {
  text: string;
  /** Theme token path, e.g. "text.accent", "text.muted", "border.accent" */
  color?: string;
}

/**
 * Get colored text segments for a session tree entry.
 * Mirrors pi's getEntryDisplayText color scheme:
 *   user       → text.accent
 *   assistant  → text.success
 *   toolResult → text.muted
 *   branch_summary → text.warning
 *   compaction → border.accent
 *   model_change → text.dim
 *   other      → text.dim
 */
export function getEntrySegments(entry: SessionTreeEntry): TextSegment[] {
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
        const tr = msg as { toolResult?: { name?: string } };
        return [{ text: `[${tr.toolResult?.name ?? "tool"}]`, color: "text.muted" }];
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

// ============================================================================
// Tree flattening — ported from pi's TreeList.flattenTree
// ============================================================================

/** Gutter info: position (displayIndent level) and whether to show │ */
export interface GutterInfo {
  position: number;
  show: boolean;
}

/** Flattened tree node for display */
export interface FlatTreeEntry {
  node: SessionTreeNode;
  /** Display indentation level */
  indent: number;
  /** Whether to show connector (├─ or └─) */
  showConnector: boolean;
  /** If showConnector, true = last sibling (└─), false = not last (├─) */
  isLast: boolean;
  /** Gutter info for each ancestor branch point */
  gutters: GutterInfo[];
  /** True if this node is a root under a virtual branching root (multiple roots) */
  isVirtualRootChild: boolean;
}

/** Output item for TUI display */
export interface FlattenedTreeItem {
  id: string;
  label: string;
  description: string;
  /** Rich text segments (pi-style colored rendering). When present, SelectListView uses these. */
  segments?: TextSegment[];
  value: SessionTreeEntry;
}

/**
 * Flatten a session tree into a display-ready list with proper tree connectors.
 * Ported from pi's TreeList.flattenTree logic.
 *
 * Indentation rules:
 * - At indent 0: stay at 0 unless parent has >1 children (then +1)
 * - At indent 1: children always go to indent 2 (visual grouping of subtree)
 * - At indent 2+: stay flat for single-child chains, +1 only if parent branches
 */
export function flattenSessionTree(
  entries: SessionTreeEntry[],
  currentLeafId?: string | null,
): { flat: FlatTreeEntry[]; multipleRoots: boolean } {
  // Filter out leaf marker entries
  const filtered = entries.filter((e) => e.type !== "leaf");
  const roots = buildSessionTree(filtered);

  const result: FlatTreeEntry[] = [];
  const multipleRoots = roots.length > 1;

  // Determine which subtrees contain the current leaf (to sort current branch first)
  const containsActive = new Map<SessionTreeNode, boolean>();
  const leafId = currentLeafId ?? null;
  {
    // Pre-order traversal, then reverse for post-order
    const allNodes: SessionTreeNode[] = [];
    const preOrderStack: SessionTreeNode[] = [...roots];
    while (preOrderStack.length > 0) {
      const node = preOrderStack.pop()!;
      allNodes.push(node);
      for (let i = node.children.length - 1; i >= 0; i--) {
        preOrderStack.push(node.children[i]);
      }
    }
    // Post-order: children before parents
    for (let i = allNodes.length - 1; i >= 0; i--) {
      const node = allNodes[i];
      let has = leafId !== null && node.entry.id === leafId;
      for (const child of node.children) {
        if (containsActive.get(child)) {
          has = true;
        }
      }
      containsActive.set(node, has);
    }
  }

  // Stack: [node, indent, justBranched, showConnector, isLast, gutters, isVirtualRootChild]
  type StackItem = [SessionTreeNode, number, boolean, boolean, boolean, GutterInfo[], boolean];
  const stack: StackItem[] = [];

  // Add roots in reverse order, prioritizing the one containing the active leaf
  const orderedRoots = [...roots].sort(
    (a, b) => Number(containsActive.get(b)) - Number(containsActive.get(a)),
  );
  for (let i = orderedRoots.length - 1; i >= 0; i--) {
    const isLast = i === orderedRoots.length - 1;
    stack.push([
      orderedRoots[i],
      multipleRoots ? 1 : 0,
      multipleRoots,
      multipleRoots,
      isLast,
      [],
      multipleRoots,
    ]);
  }

  while (stack.length > 0) {
    const [node, indent, justBranched, showConnector, isLast, gutters, isVirtualRootChild] =
      stack.pop()!;

    result.push({ node, indent, showConnector, isLast, gutters, isVirtualRootChild });

    const children = node.children;
    const multipleChildren = children.length > 1;

    // Order children so the branch containing the active leaf comes first
    const orderedChildren = (() => {
      const prioritized: SessionTreeNode[] = [];
      const rest: SessionTreeNode[] = [];
      for (const child of children) {
        if (containsActive.get(child)) {
          prioritized.push(child);
        } else {
          rest.push(child);
        }
      }
      return [...prioritized, ...rest];
    })();

    // Calculate child indent
    let childIndent: number;
    if (multipleChildren) {
      childIndent = indent + 1;
    } else if (justBranched && indent > 0) {
      childIndent = indent + 1;
    } else {
      childIndent = indent;
    }

    // Build gutters for children
    const connectorDisplayed = showConnector && !isVirtualRootChild;
    // Connector is at position (displayIndent - 1)
    const currentDisplayIndent = multipleRoots ? Math.max(0, indent - 1) : indent;
    const connectorPosition = Math.max(0, currentDisplayIndent - 1);
    const childGutters: GutterInfo[] = connectorDisplayed
      ? [...gutters, { position: connectorPosition, show: !isLast }]
      : gutters;

    // Add children in reverse order (to pop in forward order)
    for (let i = orderedChildren.length - 1; i >= 0; i--) {
      const childIsLast = i === orderedChildren.length - 1;
      stack.push([
        orderedChildren[i],
        childIndent,
        multipleChildren,
        multipleChildren,
        childIsLast,
        childGutters,
        false,
      ]);
    }
  }

  return { flat: result, multipleRoots };
}

/**
 * Render a flat tree entry list into display items with connector prefixes.
 * Builds proper ├─ / └─ / │ ASCII tree art.
 *
 * Follows pi's inline style: tree connector + [label] + path marker + entry text
 * all appear in the main label field. Description holds secondary type info.
 */
export function renderFlatTree(flat: FlatTreeEntry[], multipleRoots: boolean): FlattenedTreeItem[] {
  return flat.map((flatEntry) => {
    const entry = flatEntry.node.entry as SessionTreeEntry & {
      isOnCurrentBranch?: boolean;
      isLeaf?: boolean;
    };

    // Display indent: for multiple roots, shift roots left by 1
    const displayIndent = multipleRoots ? Math.max(0, flatEntry.indent - 1) : flatEntry.indent;

    const connector =
      flatEntry.showConnector && !flatEntry.isVirtualRootChild
        ? flatEntry.isLast
          ? "└─ "
          : "├─ "
        : "";
    const connectorPosition = connector ? displayIndent - 1 : -1;

    // Build prefix char by char (same as pi's TreeList.render)
    const totalChars = displayIndent * 3;
    const prefixChars: string[] = [];
    for (let i = 0; i < totalChars; i++) {
      const level = Math.floor(i / 3);
      const posInLevel = i % 3;

      const gutter = flatEntry.gutters.find((g) => g.position === level);
      if (gutter) {
        prefixChars.push(posInLevel === 0 ? (gutter.show ? "│" : " ") : " ");
      } else if (connector && level === connectorPosition) {
        if (posInLevel === 0) {
          prefixChars.push(flatEntry.isLast ? "└" : "├");
        } else if (posInLevel === 1) {
          prefixChars.push("─");
        } else {
          prefixChars.push(" ");
        }
      } else {
        prefixChars.push(" ");
      }
    }
    const prefix = prefixChars.join("");

    // Inline markers (pi style): label, path marker, all before entry text
    const inline: string[] = [];
    if (flatEntry.node.label) inline.push(`[${flatEntry.node.label}]`);
    if (entry.isOnCurrentBranch) inline.push("●");
    if (entry.isLeaf) inline.push("◀");
    const inlineStr = inline.length > 0 ? `${inline.join(" ")} ` : "";

    // Colored content segments from getEntrySegments (pi-style role colors)
    const contentSegments = getEntrySegments(flatEntry.node.entry);

    // Build rich segments: prefix (plain) + inline markers (plain) + content (colored)
    const segments: TextSegment[] = [];
    if (prefix) segments.push({ text: prefix });
    if (inlineStr) segments.push({ text: inlineStr });
    for (const seg of contentSegments) {
      segments.push(seg);
    }

    // Plain text label (fallback for non-rich renderers)
    const contentLabel = getEntryLabel(flatEntry.node.entry);

    // Description: just type metadata (role for messages, type for others)
    const descParts: string[] = [];
    if (entry.type === "message") {
      descParts.push((entry.message as any)?.role ?? "message");
    } else {
      descParts.push(entry.type);
    }
    if (flatEntry.node.label) descParts.push(`label:${flatEntry.node.label}`);

    return {
      id: flatEntry.node.entry.id,
      label: prefix + inlineStr + contentLabel,
      segments,
      description: descParts.join(" "),
      value: flatEntry.node.entry,
    };
  });
}
