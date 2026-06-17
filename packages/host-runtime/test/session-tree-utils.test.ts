/**
 * Tests for session tree utilities: flattenSessionTree + renderFlatTree.
 * Lock down the tree rendering contract to prevent regressions.
 */

import { describe, expect, it } from "bun:test";
import type {
  BranchSummaryEntry,
  CompactionEntry,
  LabelEntry,
  LeafEntry,
  MessageEntry,
  ModelChangeEntry,
  SessionInfoEntry,
  SessionTreeEntry,
} from "piko-session";
import {
  buildSessionTree,
  flattenSessionTree,
  getEntryLabel,
  getEntrySegments,
  recalculateVisibleFlatTree,
  renderFlatTree,
} from "../src/session/session-tree-utils/index.js";

// ---- Helpers ----

function makeMessage(
  id: string,
  parentId: string | null,
  role: "user" | "assistant" | "toolResult",
  content: string,
  timestamp = "2024-01-01T00:00:00Z",
  toolResultName?: string,
): MessageEntry {
  const msg: any = {
    role,
    content:
      role === "toolResult"
        ? [{ type: "tool_result", tool_call_id: "tc1", content: [{ type: "text", text: content }] }]
        : content,
  };
  if (toolResultName) {
    msg.toolResult = { name: toolResultName };
  }
  return {
    type: "message",
    id,
    parentId,
    timestamp,
    message: msg,
  };
}

function makeBranchSummary(
  id: string,
  parentId: string,
  summary: string,
  fromId: string,
): BranchSummaryEntry {
  return {
    type: "branch_summary",
    id,
    parentId,
    timestamp: "2024-01-01T00:00:00Z",
    summary,
    fromId,
  };
}

function makeCompaction(
  id: string,
  parentId: string | null,
  summary: string,
  firstKeptEntryId: string,
  tokensBefore: number,
): CompactionEntry {
  return {
    type: "compaction",
    id,
    parentId,
    timestamp: "2024-01-01T00:00:00Z",
    summary,
    firstKeptEntryId,
    tokensBefore,
  };
}

function makeLeaf(id: string, parentId: string, targetId: string | null): LeafEntry {
  return {
    type: "leaf",
    id,
    parentId,
    timestamp: "2024-01-01T00:00:00Z",
    targetId,
  };
}

function makeSessionInfo(id: string, parentId: string | null, name?: string): SessionInfoEntry {
  return {
    type: "session_info",
    id,
    parentId,
    timestamp: "2024-01-01T00:00:00Z",
    name,
  };
}

function makeLabel(
  id: string,
  parentId: string | null,
  targetId: string,
  label?: string,
): LabelEntry {
  return {
    type: "label",
    id,
    parentId,
    timestamp: "2024-01-01T00:00:00Z",
    targetId,
    label,
  };
}

function makeModelChange(
  id: string,
  parentId: string | null,
  provider: string,
  modelId: string,
): ModelChangeEntry {
  return {
    type: "model_change",
    id,
    parentId,
    timestamp: "2024-01-01T00:00:00Z",
    provider,
    modelId,
  };
}

// Helper to extract just the label text (strip tree connectors) for assertions
function labels(items: ReturnType<typeof renderFlatTree>): string[] {
  return items.map((it) => it.label);
}

// Helper to get descriptions
function _descriptions(items: ReturnType<typeof renderFlatTree>): string[] {
  return items.map((it) => it.description);
}

// ---- Tests ----

describe("getEntryLabel", () => {
  it("returns label for user message", () => {
    const entry = makeMessage("m1", null, "user", "Hello, world!");
    expect(getEntryLabel(entry)).toBe("user: Hello, world!");
  });

  it("truncates long user messages to 120 chars", () => {
    const long = "x".repeat(200);
    const entry = makeMessage("m1", null, "user", long);
    const label = getEntryLabel(entry);
    expect(label.startsWith("user: ")).toBe(true);
    expect(label.length).toBeLessThanOrEqual(126); // "user: " + 120 chars
  });

  it("returns label for assistant message", () => {
    const entry = makeMessage("m2", "m1", "assistant", "I'll help.");
    expect(getEntryLabel(entry)).toBe("assistant: I'll help.");
  });

  it("returns tool calls placeholder for assistant with no text", () => {
    const entry: MessageEntry = {
      type: "message",
      id: "m3",
      parentId: "m2",
      timestamp: "2024-01-01T00:00:00Z",
      message: { role: "assistant", content: "" } as any,
    };
    expect(getEntryLabel(entry)).toBe("assistant: (tool calls)");
  });

  it("returns label for tool result", () => {
    const entry = makeMessage("m4", "m3", "toolResult", "ok", undefined, "read");
    expect(getEntryLabel(entry)).toBe("[read]");
  });

  it("returns label for branch summary", () => {
    const entry = makeBranchSummary("b1", "m5", "This branch adds feature X", "m2");
    expect(getEntryLabel(entry)).toBe("branch: This branch adds feature X");
  });

  it("returns label for compaction", () => {
    const entry = makeCompaction("c1", "m1", "Compacted 10 messages", "m5", 5000);
    expect(getEntryLabel(entry)).toBe("compaction: Compacted 10 messages");
  });

  it("returns label for session info with name", () => {
    const entry = makeSessionInfo("s1", null, "My Session");
    expect(getEntryLabel(entry)).toBe("title: My Session");
  });

  it("returns label for session info without name", () => {
    const entry = makeSessionInfo("s1", null);
    expect(getEntryLabel(entry)).toBe("title: (cleared)");
  });

  it("returns label for model change", () => {
    const entry = makeModelChange("mc1", null, "openai", "gpt-4");
    expect(getEntryLabel(entry)).toBe("model: openai/gpt-4");
  });

  it("returns type string for unknown entry type", () => {
    const entry = { type: "unknown_type", id: "u1", parentId: null, timestamp: "" } as any;
    expect(getEntryLabel(entry)).toBe("unknown_type");
  });
});

describe("getEntrySegments", () => {
  it("returns colored segments for user message", () => {
    const entry = makeMessage("m1", null, "user", "Hello");
    const segs = getEntrySegments(entry);
    expect(segs).toHaveLength(2);
    expect(segs[0]).toEqual({ text: "user: ", color: "text.accent" });
    expect(segs[1]).toEqual({ text: "Hello" });
  });

  it("returns colored segments for assistant message", () => {
    const entry = makeMessage("m2", "m1", "assistant", "I'll help.");
    const segs = getEntrySegments(entry);
    expect(segs[0]).toEqual({ text: "assistant: ", color: "text.success" });
    expect(segs[1]).toEqual({ text: "I'll help." });
  });

  it("returns tool calls placeholder for assistant with no text", () => {
    const entry: MessageEntry = {
      type: "message",
      id: "m3",
      parentId: "m2",
      timestamp: "2024-01-01T00:00:00Z",
      message: { role: "assistant", content: "" } as any,
    };
    const segs = getEntrySegments(entry);
    expect(segs).toHaveLength(1);
    expect(segs[0]).toEqual({ text: "assistant: (tool calls)", color: "text.success" });
  });

  it("returns muted segment for tool result", () => {
    const entry = makeMessage("m4", "m3", "toolResult", "ok", undefined, "read");
    const segs = getEntrySegments(entry);
    expect(segs[0]).toEqual({ text: "[read]", color: "text.muted" });
  });

  it("returns warning segments for branch summary", () => {
    const entry = makeBranchSummary("b1", "m5", "added feature", "m2");
    const segs = getEntrySegments(entry);
    expect(segs[0]).toEqual({ text: "branch: ", color: "text.warning" });
    expect(segs[1]).toEqual({ text: "added feature" });
  });

  it("returns border.accent segments for compaction", () => {
    const entry = makeCompaction("c1", "m1", "compacted 10 msgs", "m5", 5000);
    const segs = getEntrySegments(entry);
    expect(segs[0]).toEqual({ text: "compaction: ", color: "border.accent" });
  });

  it("returns dim segment for model change", () => {
    const entry = makeModelChange("mc1", null, "openai", "gpt-4");
    const segs = getEntrySegments(entry);
    expect(segs[0]).toEqual({ text: "model: openai/gpt-4", color: "text.dim" });
  });
});

describe("buildSessionTree", () => {
  it("returns empty array for empty entries", () => {
    expect(buildSessionTree([])).toEqual([]);
  });

  it("builds single root from single entry", () => {
    const entries = [makeMessage("m1", null, "user", "hi")];
    const roots = buildSessionTree(entries);
    expect(roots).toHaveLength(1);
    expect(roots[0].entry.id).toBe("m1");
    expect(roots[0].children).toEqual([]);
  });

  it("builds linear chain", () => {
    const entries = [
      makeMessage("m1", null, "user", "hi"),
      makeMessage("m2", "m1", "assistant", "hello"),
      makeMessage("m3", "m2", "user", "thanks"),
    ];
    const roots = buildSessionTree(entries);
    expect(roots).toHaveLength(1);
    expect(roots[0].entry.id).toBe("m1");
    expect(roots[0].children).toHaveLength(1);
    expect(roots[0].children[0].entry.id).toBe("m2");
    expect(roots[0].children[0].children).toHaveLength(1);
    expect(roots[0].children[0].children[0].entry.id).toBe("m3");
  });

  it("builds branched tree (fork)", () => {
    const entries = [
      makeMessage("m1", null, "user", "task"),
      makeMessage("m2", "m1", "assistant", "working"),
      // Branch 1 (main)
      makeMessage("m3", "m2", "user", "continue"),
      makeMessage("m4", "m3", "assistant", "done"),
      // Branch 2 (fork from m2)
      makeMessage("m5", "m2", "user", "alternative"),
      makeMessage("m6", "m5", "assistant", "alt done"),
    ];
    const roots = buildSessionTree(entries);
    expect(roots).toHaveLength(1);
    const m2 = roots[0].children[0]; // m2 is child of m1
    expect(m2.children).toHaveLength(2);
    // Children should be in order of entry appearance
    expect(m2.children[0].entry.id).toBe("m3");
    expect(m2.children[1].entry.id).toBe("m5");
  });

  it("resolves label entries onto target nodes", () => {
    const entries = [
      makeMessage("m1", null, "user", "hi"),
      makeLabel("l1", "m1", "m1", "My Label"),
    ];
    const roots = buildSessionTree(entries);
    expect(roots[0].label).toBe("My Label");
    expect(roots[0].labelTimestamp).toBe("2024-01-01T00:00:00Z");
  });

  it("clears labels with later empty label entries", () => {
    const entries = [
      makeMessage("m1", null, "user", "hi"),
      makeLabel("l1", "m1", "m1", "My Label"),
      makeLabel("l2", "l1", "m1"),
    ];
    const roots = buildSessionTree(entries);
    expect(roots[0].label).toBeUndefined();
  });
});

describe("flattenSessionTree", () => {
  it("returns empty flat for empty entries", () => {
    const { flat, multipleRoots } = flattenSessionTree([]);
    expect(flat).toEqual([]);
    expect(multipleRoots).toBe(false);
  });

  it("filters out leaf entries", () => {
    const entries: SessionTreeEntry[] = [
      makeMessage("m1", null, "user", "hi"),
      makeLeaf("l1", "m1", "m1"),
    ];
    const { flat } = flattenSessionTree(entries);
    expect(flat).toHaveLength(1);
    expect(flat[0].node.entry.id).toBe("m1");
  });

  it("flattens single root with no connector", () => {
    const entries = [makeMessage("m1", null, "user", "hi")];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    expect(flat).toHaveLength(1);
    expect(flat[0].indent).toBe(0);
    expect(flat[0].showConnector).toBe(false);
    expect(multipleRoots).toBe(false);
  });

  it("flattens linear chain (no branching, stays flat)", () => {
    const entries = [
      makeMessage("m1", null, "user", "hi"),
      makeMessage("m2", "m1", "assistant", "hello"),
      makeMessage("m3", "m2", "user", "thanks"),
    ];
    const { flat } = flattenSessionTree(entries);
    expect(flat).toHaveLength(3);
    // All should be at indent 0 since single-child chain stays flat
    expect(flat[0].indent).toBe(0);
    expect(flat[1].indent).toBe(0);
    expect(flat[2].indent).toBe(0);
    // Only first connector shown, others are suppressed (showConnector=false for single-child chain)
    expect(flat[0].showConnector).toBe(false);
    expect(flat[1].showConnector).toBe(false);
    expect(flat[2].showConnector).toBe(false);
  });

  it("flattens branched tree with proper indentation", () => {
    const entries = [
      makeMessage("m1", null, "user", "task"),
      makeMessage("m2", "m1", "assistant", "working"),
      makeMessage("m3", "m2", "user", "branch a"),
      makeMessage("m4", "m3", "assistant", "done a"),
      makeMessage("m5", "m2", "user", "branch b"),
      makeMessage("m6", "m5", "assistant", "done b"),
    ];
    const { flat } = flattenSessionTree(entries);
    // m1 (root) → m2 → m3 → m4 (branch a) + m5 → m6 (branch b)
    // Pi indent rules:
    //   indent 0 + branching → children at indent 1
    //   indent 1 + justBranched → children at indent 2
    //   indent 2+ single child → stays flat
    expect(flat).toHaveLength(6);

    const ids = flat.map((f) => f.node.entry.id);
    expect(ids).toEqual(["m1", "m2", "m3", "m4", "m5", "m6"]);

    expect(flat[0].indent).toBe(0); // m1 root, no connector
    expect(flat[1].indent).toBe(0); // m2 single child of m1 → stays flat, no connector

    expect(flat[2].indent).toBe(1); // m3 first child of m2 (branch point) → indent 1
    expect(flat[2].showConnector).toBe(true);
    expect(flat[2].isLast).toBe(false); // m3 is first child

    expect(flat[3].indent).toBe(2); // m4 child of m3 (indent 1 + justBranched) → indent 2
    expect(flat[3].showConnector).toBe(false); // single child of m3 → stays flat

    expect(flat[4].indent).toBe(1); // m5 second child of m2 → indent 1
    expect(flat[4].showConnector).toBe(true);
    expect(flat[4].isLast).toBe(true); // m5 is last child

    expect(flat[5].indent).toBe(2); // m6 child of m5 → indent 2
    expect(flat[5].showConnector).toBe(false); // single child → stays flat
  });

  it("marks nodes with isVirtualRootChild for multiple roots", () => {
    // Two independent roots (e.g., two sessions merged)
    const entries = [
      makeMessage("r1", null, "user", "session 1"),
      makeMessage("r2", "r1", "assistant", "reply 1"),
      makeMessage("r3", null, "user", "session 2"),
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    expect(multipleRoots).toBe(true);
    // Both roots have indent 1 (children of virtual root), marked isVirtualRootChild
    expect(flat[0].indent).toBe(1);
    expect(flat[0].isVirtualRootChild).toBe(true);
    expect(flat[0].showConnector).toBe(true);
    expect(flat[0].isLast).toBe(false); // first root, not last

    // r2: child of r1 at indent 1 + justBranched → indent 2
    expect(flat[1].indent).toBe(2);
    expect(flat[1].isVirtualRootChild).toBe(false);
    expect(flat[1].showConnector).toBe(false); // single child → no connector

    expect(flat[2].indent).toBe(1);
    expect(flat[2].isVirtualRootChild).toBe(true);
    expect(flat[2].isLast).toBe(true); // second root, is last
  });

  it("sorts active branch first", () => {
    const entries = [
      makeMessage("m1", null, "user", "task"),
      makeMessage("m2", "m1", "assistant", "working"),
      makeMessage("m3", "m2", "user", "branch a"),
      makeMessage("m4", "m2", "user", "branch b"),
    ];
    // m4 is the current leaf → its branch should come first
    const { flat } = flattenSessionTree(entries, "m4");
    const ids = flat.map((f) => f.node.entry.id);
    // m4's branch (m4) should appear before m3's branch (m3)
    expect(ids.indexOf("m4")).toBeLessThan(ids.indexOf("m3"));
  });
});

describe("renderFlatTree", () => {
  it("returns empty for empty flat", () => {
    expect(renderFlatTree([], false)).toEqual([]);
  });

  it("renders single root without connector", () => {
    const entries = [makeMessage("m1", null, "user", "hi")];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);
    expect(items).toHaveLength(1);
    expect(items[0].label).toBe("user: hi");
    // message entries show role in description, not type
    expect(items[0].description).toBe("user");
  });

  it("renders linear chain (flat, no connectors)", () => {
    const entries = [
      makeMessage("m1", null, "user", "hi"),
      makeMessage("m2", "m1", "assistant", "hello"),
      makeMessage("m3", "m2", "user", "bye"),
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);
    // Linear chain stays flat (indent 0), no connectors
    expect(labels(items)).toEqual(["user: hi", "assistant: hello", "user: bye"]);
  });

  it("renders branched tree with ├─ / └─ connectors", () => {
    const entries = [
      makeMessage("m1", null, "user", "task"),
      makeMessage("m2", "m1", "assistant", "working"),
      makeMessage("m3", "m2", "user", "branch a"),
      makeMessage("m4", "m3", "assistant", "done a"),
      makeMessage("m5", "m2", "user", "branch b"),
      makeMessage("m6", "m5", "assistant", "done b"),
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);

    // m3 is first child of m2 → ├─
    expect(items[2].label).toContain("├─ user: branch a");
    // m3's label should have gutter (│) from parent m2 being not-last:
    // m3 indent=2, m2 is not-last → gutter at position 0 shows │
    // Actually, m2 is at indent 0 and has no connector, so no gutter added.
    // Let me check the actual output...

    // m5 is last child of m2 → └─
    expect(items[4].label).toContain("└─ user: branch b");
  });

  it("renders tool results using the originating assistant tool call", () => {
    const entries: SessionTreeEntry[] = [
      {
        type: "message",
        id: "m1",
        parentId: null,
        timestamp: "2024-01-01T00:00:00Z",
        message: {
          role: "assistant",
          content: [
            { type: "toolCall", id: "tc-1", name: "read", arguments: { path: "README.md" } },
          ],
        } as any,
      },
      {
        type: "message",
        id: "m2",
        parentId: "m1",
        timestamp: "2024-01-01T00:00:01Z",
        message: {
          role: "toolResult",
          toolCallId: "tc-1",
          content: [{ type: "text", text: "ok" }],
        } as any,
      },
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);

    expect(items[1].label).toBe("[read: README.md]");
    expect(items[1].segments?.map((segment) => segment.text).join("")).toBe("[read: README.md]");
  });

  it("renders tool result args when the originating assistant call is filtered out", () => {
    const entries: SessionTreeEntry[] = [
      {
        type: "message",
        id: "m1",
        parentId: null,
        timestamp: "2024-01-01T00:00:00Z",
        message: {
          role: "assistant",
          content: [
            {
              type: "toolCall",
              id: "tc-1",
              name: "read",
              arguments: { path: "packages/host-runtime/src/session/session-tree-utils.ts" },
            },
          ],
        } as any,
      },
      {
        type: "message",
        id: "m2",
        parentId: "m1",
        timestamp: "2024-01-01T00:00:01Z",
        message: {
          role: "toolResult",
          toolCallId: "tc-1",
          content: [{ type: "text", text: "ok" }],
        } as any,
      },
    ];
    const { flat } = flattenSessionTree(entries);
    const visibleFlat = flat.filter((entry) => entry.node.entry.id === "m2");
    const recalculated = recalculateVisibleFlatTree(visibleFlat, flat);
    const items = renderFlatTree(recalculated.flat, recalculated.multipleRoots, flat);

    expect(items[0].label).toBe("[read: packages/host-runtime/src/session/session-tree-utils.ts]");
  });

  it("renders multiple roots without connectors (isVirtualRootChild suppresses)", () => {
    const entries = [
      makeMessage("r1", null, "user", "first"),
      makeMessage("r2", null, "user", "second"),
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);
    expect(multipleRoots).toBe(true);
    // isVirtualRootChild=true suppresses connectors in renderFlatTree
    // displayIndent = max(0, 1-1) = 0 → no prefix
    expect(items[0].label).toBe("user: first");
    expect(items[1].label).toBe("user: second");
  });

  it("shows branch markers when isOnCurrentBranch / isLeaf are set", () => {
    // Use entries with getTree()-style enriched properties
    const entries: any[] = [
      {
        type: "message",
        id: "m1",
        parentId: null,
        timestamp: "2024-01-01T00:00:00Z",
        message: { role: "user", content: "hi" },
        isOnCurrentBranch: true,
      },
      {
        type: "message",
        id: "m2",
        parentId: "m1",
        timestamp: "2024-01-01T00:00:00Z",
        message: { role: "assistant", content: "hello" },
        isOnCurrentBranch: true,
        isLeaf: true,
      },
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);
    // m2 should show both active-path and leaf markers inline in the label.
    expect(items[1].label).toContain("•");
    expect(items[1].label).toContain("◀");
  });

  it("shows label brackets for nodes with labels", () => {
    const entries: SessionTreeEntry[] = [
      makeMessage("m1", null, "user", "hi"),
      makeLabel("l1", "m1", "m1", "Test Label"),
    ];
    const { flat } = flattenSessionTree(entries);
    // buildSessionTree should have set the label on m1
    expect(flat[0].node.label).toBe("Test Label");
    const items = renderFlatTree(flat, false);
    // Label appears inline in the rendered label (pi style), and as metadata in description
    expect(items[0].label).toContain("[Test Label]");
    expect(items[0].description).toContain("label:Test Label");
  });
});

describe("recalculateVisibleFlatTree", () => {
  it("reattaches descendants to nearest visible ancestor after filtering", () => {
    const entries: SessionTreeEntry[] = [
      makeMessage("m1", null, "user", "root"),
      makeMessage("m2", "m1", "assistant", "hidden middle"),
      makeMessage("m3", "m2", "user", "branch a"),
      makeMessage("m4", "m2", "user", "branch b"),
    ];
    const { flat } = flattenSessionTree(entries);
    const visible = flat.filter((node) => node.node.entry.id !== "m2");
    const recalculated = recalculateVisibleFlatTree(visible, flat);
    const items = renderFlatTree(recalculated.flat, recalculated.multipleRoots);

    expect(labels(items)).toEqual(["user: root", "├─ user: branch a", "└─ user: branch b"]);
  });
});

describe("flattenSessionTree + renderFlatTree round-trip", () => {
  it("handles branch_summary entries", () => {
    const entries: SessionTreeEntry[] = [
      makeMessage("m1", null, "user", "task"),
      makeMessage("m2", "m1", "assistant", "working"),
      makeBranchSummary("bs1", "m2", "branched here", "m2"),
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);
    expect(items).toHaveLength(3);
    expect(items[2].label).toContain("branch: branched here");
  });

  it("handles compaction entries", () => {
    const entries: SessionTreeEntry[] = [
      makeMessage("m1", null, "user", "hi"),
      makeCompaction("c1", "m1", "compacted 5 msgs", "m1", 3000),
      makeMessage("m2", "c1", "assistant", "after compaction"),
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);
    expect(items).toHaveLength(3);
    expect(items[1].label).toContain("compaction: compacted 5 msgs");
  });

  it("handles model_change entries", () => {
    const entries: SessionTreeEntry[] = [
      makeModelChange("mc1", null, "anthropic", "claude-3"),
      makeMessage("m1", "mc1", "user", "hi"),
    ];
    const { flat, multipleRoots } = flattenSessionTree(entries);
    const items = renderFlatTree(flat, multipleRoots);
    expect(items).toHaveLength(2);
    expect(items[0].label).toContain("model: anthropic/claude-3");
  });
});
