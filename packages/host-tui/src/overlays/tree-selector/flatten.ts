import type { SessionTreeNode } from "piko-host-runtime";
import type { FlatNode, GutterInfo } from "./types.js";

interface ToolCallInfo {
  name: string;
  arguments: Record<string, unknown>;
}

export function flattenTree(
  roots: SessionTreeNode[],
  currentLeafId: string | null,
): { nodes: FlatNode[]; toolCallMap: Map<string, ToolCallInfo> } {
  const toolCallMap = new Map<string, ToolCallInfo>();
  const result: FlatNode[] = [];
  const multipleRoots = roots.length > 1;

  const containsActive = new Map<SessionTreeNode, boolean>();
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
      let has = currentLeafId !== null && node.entry.id === currentLeafId;
      for (const child of node.children) {
        if (containsActive.get(child)) has = true;
      }
      containsActive.set(node, has);
    }
  }

  const orderedRoots = [...roots].sort(
    (a, b) => Number(containsActive.get(b)) - Number(containsActive.get(a)),
  );

  type StackItem = [SessionTreeNode, number, boolean, boolean, boolean, GutterInfo[], boolean];
  const stack: StackItem[] = [];
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

    const entry = node.entry;
    if (entry.type === "message" && entry.message.role === "assistant") {
      const content = entry.message.content;
      if (Array.isArray(content)) {
        for (const block of content) {
          if (
            typeof block === "object" &&
            block !== null &&
            "type" in block &&
            block.type === "toolCall"
          ) {
            const tc = block as ToolCallInfo & { id: string };
            toolCallMap.set(tc.id, { name: tc.name, arguments: tc.arguments });
          }
        }
      }
    }

    result.push({ node, indent, showConnector, isLast, gutters, isVirtualRootChild });

    const children = node.children;
    const multipleChildren = children.length > 1;

    const orderedChildren = (() => {
      const prioritized: SessionTreeNode[] = [];
      const rest: SessionTreeNode[] = [];
      for (const child of children) {
        if (containsActive.get(child)) prioritized.push(child);
        else rest.push(child);
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

  return { nodes: result, toolCallMap };
}
