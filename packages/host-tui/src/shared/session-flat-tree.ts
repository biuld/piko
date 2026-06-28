// ============================================================================
// Session flat-tree utilities — flattening, searching, rendering.
// Pure functions operating on hostd-provided data.
// ============================================================================

import { collectToolCalls } from "./session-content.js";
import { getEntryLabel, getEntrySegments } from "./session-display.js";
import { buildSessionTree } from "./session-tree.js";
import type { SessionTreeEntry, SessionTreeNode, TextSegment } from "./types.js";

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

export function flattenSessionTree(
  entries: SessionTreeEntry[],
  currentLeafId?: string | null,
): { flat: FlatTreeEntry[]; multipleRoots: boolean } {
  const filtered = entries.filter((e) => e.type !== "leaf");
  const roots = buildSessionTree(filtered);

  const result: FlatTreeEntry[] = [];
  const multipleRoots = roots.length > 1;
  const containsActive = new Map<SessionTreeNode, boolean>();
  const leafId = currentLeafId ?? null;
  {
    const allNodes: SessionTreeNode[] = [];
    const preOrderStack: SessionTreeNode[] = [...roots];
    while (preOrderStack.length > 0) {
      const node = preOrderStack.pop()!;
      allNodes.push(node);
      for (let i = node.children.length - 1; i >= 0; i--) {
        preOrderStack.push(node.children[i]);
      }
    }
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

  type StackItem = [SessionTreeNode, number, boolean, boolean, boolean, GutterInfo[], boolean];
  const stack: StackItem[] = [];

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

    let childIndent: number;
    if (multipleChildren) {
      childIndent = indent + 1;
    } else if (justBranched && indent > 0) {
      childIndent = indent + 1;
    } else {
      childIndent = indent;
    }

    const connectorDisplayed = showConnector && !isVirtualRootChild;
    const currentDisplayIndent = multipleRoots ? Math.max(0, indent - 1) : indent;
    const connectorPosition = Math.max(0, currentDisplayIndent - 1);
    const childGutters: GutterInfo[] = connectorDisplayed
      ? [...gutters, { position: connectorPosition, show: !isLast }]
      : gutters;

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

export function recalculateVisibleFlatTree(
  visibleFlat: FlatTreeEntry[],
  fullFlat: FlatTreeEntry[] = visibleFlat,
): {
  flat: FlatTreeEntry[];
  multipleRoots: boolean;
} {
  if (visibleFlat.length === 0) return { flat: [], multipleRoots: false };

  const visibleIds = new Set(visibleFlat.map((entry) => entry.node.entry.id));
  const entryMap = new Map<string, FlatTreeEntry>();
  for (const flatEntry of fullFlat) {
    entryMap.set(flatEntry.node.entry.id, flatEntry);
  }

  const findVisibleAncestor = (nodeId: string): string | null => {
    let currentId = entryMap.get(nodeId)?.node.entry.parentId ?? null;
    while (currentId !== null) {
      if (visibleIds.has(currentId)) return currentId;
      currentId = entryMap.get(currentId)?.node.entry.parentId ?? null;
    }
    return null;
  };

  const visibleChildren = new Map<string | null, string[]>();
  visibleChildren.set(null, []);
  for (const flatEntry of visibleFlat) {
    const nodeId = flatEntry.node.entry.id;
    const parentId = findVisibleAncestor(nodeId);
    if (!visibleChildren.has(parentId)) visibleChildren.set(parentId, []);
    visibleChildren.get(parentId)!.push(nodeId);
  }

  const multipleRoots = (visibleChildren.get(null) ?? []).length > 1;
  const flatById = new Map(visibleFlat.map((flatEntry) => [flatEntry.node.entry.id, flatEntry]));
  const result: FlatTreeEntry[] = [];

  type StackItem = [string, number, boolean, boolean, boolean, GutterInfo[], boolean];
  const stack: StackItem[] = [];
  const rootIds = visibleChildren.get(null) ?? [];
  for (let i = rootIds.length - 1; i >= 0; i--) {
    const isLast = i === rootIds.length - 1;
    stack.push([
      rootIds[i],
      multipleRoots ? 1 : 0,
      multipleRoots,
      multipleRoots,
      isLast,
      [],
      multipleRoots,
    ]);
  }

  while (stack.length > 0) {
    const [nodeId, indent, justBranched, showConnector, isLast, gutters, isVirtualRootChild] =
      stack.pop()!;
    const flatEntry = flatById.get(nodeId);
    if (!flatEntry) continue;

    const updated: FlatTreeEntry = {
      ...flatEntry,
      indent,
      showConnector,
      isLast,
      gutters,
      isVirtualRootChild,
    };
    result.push(updated);

    const children = visibleChildren.get(nodeId) ?? [];
    const multipleChildren = children.length > 1;
    let childIndent: number;
    if (multipleChildren) {
      childIndent = indent + 1;
    } else if (justBranched && indent > 0) {
      childIndent = indent + 1;
    } else {
      childIndent = indent;
    }

    const connectorDisplayed = showConnector && !isVirtualRootChild;
    const currentDisplayIndent = multipleRoots ? Math.max(0, indent - 1) : indent;
    const connectorPosition = Math.max(0, currentDisplayIndent - 1);
    const childGutters: GutterInfo[] = connectorDisplayed
      ? [...gutters, { position: connectorPosition, show: !isLast }]
      : gutters;

    for (let i = children.length - 1; i >= 0; i--) {
      const childIsLast = i === children.length - 1;
      stack.push([
        children[i],
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

export function renderFlatTree(
  flat: FlatTreeEntry[],
  multipleRoots: boolean,
  toolSourceFlat: FlatTreeEntry[] = flat,
): FlattenedTreeItem[] {
  const toolCalls = collectToolCalls(toolSourceFlat.map((flatEntry) => flatEntry.node.entry));
  return flat.map((flatEntry) => {
    const entry = flatEntry.node.entry as SessionTreeEntry & {
      isOnCurrentBranch?: boolean;
      isLeaf?: boolean;
    };

    const displayIndent = multipleRoots ? Math.max(0, flatEntry.indent - 1) : flatEntry.indent;
    const connector =
      flatEntry.showConnector && !flatEntry.isVirtualRootChild
        ? flatEntry.isLast
          ? "└─ "
          : "├─ "
        : "";
    const connectorPosition = connector ? displayIndent - 1 : -1;

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

    const labelMarker = flatEntry.node.label ? `[${flatEntry.node.label}] ` : "";
    const pathMarker = entry.isOnCurrentBranch ? "• " : "";
    const leafMarker = entry.isLeaf ? "◀ " : "";
    const inlineStr = `${labelMarker}${pathMarker}${leafMarker}`;
    const contentSegments = getEntrySegments(flatEntry.node.entry, toolCalls);

    const segments: TextSegment[] = [];
    if (prefix) segments.push({ text: prefix });
    if (labelMarker) segments.push({ text: labelMarker, color: "text.warning" });
    if (pathMarker) segments.push({ text: pathMarker, color: "text.accent" });
    if (leafMarker) segments.push({ text: leafMarker, color: "text.accent" });
    for (const seg of contentSegments) {
      segments.push(seg);
    }

    const contentLabel = getEntryLabel(flatEntry.node.entry, toolCalls);
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
