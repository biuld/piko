// ============================================================================
// Transcript reconciliation unit tests
// ============================================================================

import { describe, expect, it } from "bun:test";
import type { Message } from "piko-host-runtime";
import type { TuiMessageViewModel } from "../src/state/state.js";
import {
  finalizeProjection,
  reconcileLegacyTranscript as reconcileTranscript,
} from "../src/timeline/transcript-reconcile.js";
import type { TimelineItem } from "../src/timeline/types.js";

let idSeq = 0;
function makeCreateId(): () => string {
  return () => `msg-test-${++idSeq}`;
}

function makeMsg(
  id: string,
  role: "user" | "assistant" | "tool",
  text: string,
  opts?: Partial<TuiMessageViewModel>,
): TuiMessageViewModel {
  return { id, role, text, ...opts };
}

describe("reconcileTranscript", () => {
  it("preserves existing IDs for matching messages", () => {
    const existing: TuiMessageViewModel[] = [makeMsg("existing-1", "user", "Hello")];
    const canonical: Message[] = [{ role: "user", content: "Hello" } as any];

    const result = reconcileTranscript(canonical, existing, [], {
      createMessageId: makeCreateId(),
    });
    // Should reuse existing ID when content matches
    expect(result.transcript[0].id).toBe("existing-1");
  });

  it("assigns new IDs for new messages", () => {
    const existing: TuiMessageViewModel[] = [];
    const canonical: Message[] = [
      { role: "user", content: "Hello" } as any,
      { role: "assistant", content: "Hi there!" } as any,
    ];

    const result = reconcileTranscript(canonical, existing, [], {
      createMessageId: makeCreateId(),
    });
    expect(result.transcript).toHaveLength(2);
    expect(result.transcript[0].id).toMatch(/^msg-test-\d+$/);
    expect(result.transcript[1].id).toMatch(/^msg-test-\d+$/);
    expect(result.transcript[0].id).not.toBe(result.transcript[1].id);
  });

  it("produces unique sequential message IDs", () => {
    const existing: TuiMessageViewModel[] = [];
    const canonical: Message[] = [
      { role: "user", content: "A" } as any,
      { role: "user", content: "B" } as any,
      { role: "user", content: "C" } as any,
    ];

    const result = reconcileTranscript(canonical, existing, [], {
      createMessageId: makeCreateId(),
    });
    const ids = result.transcript.map((m) => m.id);
    const unique = new Set(ids);
    expect(unique.size).toBe(3);
  });

  it("merges streaming assistant with completed assistant", () => {
    const existing: TuiMessageViewModel[] = [
      { id: "stream-msg", role: "assistant", text: "Hello ", isStreaming: true },
    ];
    const canonical: Message[] = [{ role: "assistant", content: "Hello world!" } as any];

    const result = reconcileTranscript(canonical, existing, [], {
      createMessageId: makeCreateId(),
    });
    expect(result.transcript).toHaveLength(1);
    expect(result.transcript[0].id).toBe("stream-msg");
    expect(result.transcript[0].text).toBe("Hello world!");
    expect(result.transcript[0].isStreaming).toBe(false);
  });

  it("creates tool items with stable IDs from toolCallId", () => {
    const canonical: Message[] = [
      {
        role: "assistant",
        content: [
          { type: "text", text: "Let me check" },
          { type: "toolCall", id: "tc-1", name: "read", arguments: { path: "/foo" } },
        ],
      } as any,
      {
        role: "toolResult",
        toolCallId: "tc-1",
        content: "file contents",
        isError: false,
      } as any,
    ];

    const result = reconcileTranscript(canonical, [], [], { createMessageId: makeCreateId() });
    // Should have: assistant message + tool message
    expect(result.transcript.length).toBeGreaterThanOrEqual(2);

    const toolMsgs = result.transcript.filter((m) => m.role === "tool");
    expect(toolMsgs.length).toBeGreaterThanOrEqual(1);
    expect(toolMsgs[0].toolBlock?.toolCallId).toBe("tc-1");
    expect(toolMsgs[0].toolBlock?.status).toBe("success");
  });

  it("reuses existing tool IDs on reconcile", () => {
    const existing: TuiMessageViewModel[] = [
      {
        id: "old-tool-id",
        role: "tool",
        text: "",
        toolBlock: {
          toolCallId: "tc-1",
          name: "read",
          args: {},
          status: "running" as const,
          isCollapsed: false,
        },
      },
    ];
    const existingTimeline: TimelineItem[] = [
      {
        id: "tool:tc-1",
        kind: "tool-call" as const,
        text: "",
        kindDisplay: "Tool",
        isStreaming: false,
        createdAt: Date.now(),
      } as any,
    ];

    const canonical: Message[] = [
      {
        role: "toolResult",
        toolCallId: "tc-1",
        content: "done",
        isError: false,
      } as any,
    ];

    const result = reconcileTranscript(canonical, existing, existingTimeline, {
      createMessageId: makeCreateId(),
    });
    expect(result.transcript).toHaveLength(1);
    expect(result.transcript[0].id).toBe("old-tool-id");
    expect(result.transcript[0].toolBlock?.status).toBe("success");
  });

  it("generates timeline items from reconciled transcripts", () => {
    const canonical: Message[] = [
      { role: "user", content: "Hello" } as any,
      { role: "assistant", content: "Hi!" } as any,
    ];

    const result = reconcileTranscript(canonical, [], [], { createMessageId: makeCreateId() });
    expect(result.timelineItems).toHaveLength(2);
    expect(result.timelineItems[0].kind).toBe("user-message");
    expect(result.timelineItems[1].kind).toBe("assistant-message");
  });

  it("preserves existing timeline items when canonical transcript is empty", () => {
    const existingTranscript: TuiMessageViewModel[] = [makeMsg("existing-1", "user", "Hello")];
    const existingTimeline: TimelineItem[] = [
      {
        id: "msg:existing-1",
        kind: "user-message",
        text: "Hello",
        kindDisplay: "User",
        isStreaming: false,
        createdAt: 1,
      } as any,
    ];

    const result = reconcileTranscript([], existingTranscript, existingTimeline, {
      createMessageId: makeCreateId(),
    });

    // Transcript preserved
    expect(result.transcript).toHaveLength(1);
    expect(result.transcript[0].id).toBe("existing-1");
    // Timeline items preserved — must not be cleared
    expect(result.timelineItems).toHaveLength(1);
    expect(result.timelineItems[0].id).toBe("msg:existing-1");
  });
});

describe("finalizeProjection", () => {
  it("merges a canonical tool result into the existing tool item", () => {
    const projection = {
      orderedIds: ["msg:assistant-1", "tool:tc-1"],
      itemsById: {
        "msg:assistant-1": {
          id: "msg:assistant-1",
          kind: "assistant-message" as const,
        },
        "tool:tc-1": {
          id: "tool:tc-1",
          kind: "tool-result" as const,
          toolCallId: "tc-1",
          toolStatus: "success" as const,
          toolResult: "live result",
        },
      },
      lastAppliedSeqByRun: {},
      pendingTools: {},
    };
    const canonical: Message[] = [
      {
        role: "assistant",
        content: [{ type: "toolCall", id: "tc-1", name: "read", arguments: {} }],
      } as any,
      {
        role: "toolResult",
        toolCallId: "tc-1",
        toolName: "read",
        content: [{ type: "text", text: "final result" }],
        details: "final result",
        isError: false,
      } as any,
    ];

    const result = finalizeProjection(projection, canonical);
    expect(result.projection.itemsById["tool:tc-1"].toolResult).toBe("final result");
    expect(result.projection.itemsById["tool:tc-1"].text).toBe("final result");
    expect(result.projection.itemsById["tool:tc-1"].toolStatus).toBe("success");
  });

  it("never reassigns message identity or kind from canonical array position", () => {
    const projection = {
      orderedIds: ["msg:summary-1", "msg:assistant-1"],
      itemsById: {
        "msg:summary-1": {
          id: "msg:summary-1",
          kind: "compaction-summary" as const,
          text: "summary",
        },
        "msg:assistant-1": {
          id: "msg:assistant-1",
          kind: "assistant-stream" as const,
          text: "live answer",
          isStreaming: true,
        },
      },
      lastAppliedSeqByRun: {},
      pendingTools: {},
    };

    const result = finalizeProjection(projection, [
      { role: "user", content: "context adapter message" } as any,
      { role: "assistant", content: [{ type: "text", text: "canonical answer" }] } as any,
    ]);

    expect(result.projection.orderedIds).toEqual(projection.orderedIds);
    expect(result.projection.itemsById["msg:summary-1"].kind).toBe("compaction-summary");
    expect(result.projection.itemsById["msg:assistant-1"].text).toBe("live answer");
    expect(result.projection.itemsById["msg:assistant-1"].kind).toBe("assistant-message");
  });
});
