import type { Message } from "piko-engine-protocol";
import type { SessionEntry } from "piko-host-runtime";

// ---- Tree node ----

export interface SessionTreeNode {
  entry: SessionEntry;
  children: SessionTreeNode[];
  depth: number;
  isLeaf: boolean;
  isOnCurrentBranch: boolean;
}

/**
 * Build a tree from flat entries using parentId links.
 */
export function buildSessionTree(
  entries: (SessionEntry & { isLeaf: boolean; isOnCurrentBranch: boolean })[],
): SessionTreeNode[] {
  const byId = new Map<string, SessionTreeNode>();
  const roots: SessionTreeNode[] = [];

  // First pass: create nodes
  for (const entry of entries) {
    byId.set(entry.id, {
      entry,
      children: [],
      depth: 0,
      isLeaf: entry.isLeaf,
      isOnCurrentBranch: entry.isOnCurrentBranch,
    });
  }

  // Second pass: link children
  for (const entry of entries) {
    const node = byId.get(entry.id)!;
    if (entry.parentId && byId.has(entry.parentId)) {
      byId.get(entry.parentId)!.children.push(node);
    } else {
      roots.push(node);
    }
  }

  // Third pass: calculate depths
  for (const root of roots) {
    assignDepths(root, 0);
  }

  return roots;
}

function assignDepths(node: SessionTreeNode, depth: number): void {
  node.depth = depth;
  for (const child of node.children) {
    assignDepths(child, depth + 1);
  }
}

// ---- Entry display helpers ----

export function getEntryLabel(entry: SessionEntry): string {
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
        return `[${msg.toolName}]`;
      }
      return "[message]";
    }
    case "model_change":
      return `model: ${entry.modelId}`;
    case "session_info":
      return entry.name ? `title: ${entry.name}` : "title: (cleared)";
    default:
      return "";
  }
}

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

export function getSearchableText(node: SessionTreeNode): string {
  const entry = node.entry;
  switch (entry.type) {
    case "message":
      return `${entry.message.role} ${extractTextContent(entry.message)}`;
    case "model_change":
      return `model ${entry.modelId}`;
    case "session_info":
      return `title ${entry.name ?? ""}`;
    default:
      return "";
  }
}

// ---- Flattened node (with display info) ----

export interface FlatNode {
  node: SessionTreeNode;
  /** Display indent level (each level = 3 spaces) */
  indent: number;
  showConnector: boolean;
  isLast: boolean;
  gutters: GutterInfo[];
}

interface GutterInfo {
  position: number;
  show: boolean;
}

export function flattenTree(roots: SessionTreeNode[]): FlatNode[] {
  const result: FlatNode[] = [];
  const multipleRoots = roots.length > 1;

  // Stack: [node, indent, showConnector, isLast, gutters]
  type StackItem = [SessionTreeNode, number, boolean, boolean, GutterInfo[]];
  const stack: StackItem[] = [];

  for (let i = roots.length - 1; i >= 0; i--) {
    const isLast = i === roots.length - 1;
    stack.push([roots[i], multipleRoots ? 1 : 0, multipleRoots, isLast, []]);
  }

  while (stack.length > 0) {
    const [node, indent, showConnector, isLast, gutters] = stack.pop()!;
    result.push({ node, indent, showConnector, isLast, gutters });

    const children = node.children;
    const hasMultiple = children.length > 1;

    // Build child gutters
    const currentDisplayIndent = multipleRoots ? Math.max(0, indent - 1) : indent;
    const connectorPosition = Math.max(0, currentDisplayIndent - 1);
    const childGutters: GutterInfo[] = showConnector
      ? [...gutters, { position: connectorPosition, show: !isLast }]
      : gutters;

    // Add children in reverse order
    for (let i = children.length - 1; i >= 0; i--) {
      const childIsLast = i === children.length - 1;
      stack.push([children[i], indent + 1, hasMultiple, childIsLast, childGutters]);
    }
  }

  return result;
}
