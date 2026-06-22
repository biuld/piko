// ============================================================================
// TUI Reducer unit tests
// ============================================================================

import { describe, expect, it } from "bun:test";
import type { Model } from "@earendil-works/pi-ai";
import type { ModelProviderConfig } from "piko-orchestrator-protocol";
import type { TuiEvent } from "../src/state/events.js";
import { tuiReducer } from "../src/state/reducers/index.js";
import { selectStatus } from "../src/state/selectors.js";
import { createDefaultTuiState } from "../src/state/state.js";

function makeState() {
  const model: Model<string> = {
    id: "test-model",
    provider: "test-provider",
    label: "Test Model",
  } as any;
  const providerConfig: ModelProviderConfig = {
    provider: "test-provider",
    auth: { type: "api_key", key: "test-key" },
  } as any;
  return createDefaultTuiState(model, providerConfig, "/test/cwd");
}

describe("tuiReducer", () => {
  describe("user_submitted", () => {
    it("adds user message and transitions to running", () => {
      const state = makeState();
      const event: TuiEvent = { type: "user_submitted", text: "hello" };
      const next = tuiReducer(state, event);
      expect(next.transcript).toHaveLength(1);
      expect(next.transcript[0].role).toBe("user");
      expect(next.transcript[0].text).toBe("hello");
      expect(next.stream.status).toBe("running");
    });

    it("appends timeline item for user message", () => {
      const state = makeState();
      const event: TuiEvent = { type: "user_submitted", text: "hello" };
      const next = tuiReducer(state, event);
      expect(next.timeline.items).toHaveLength(1);
      expect(next.timeline.items[0].id).toBe(
        next.transcript[0].id.startsWith("msg:")
          ? next.transcript[0].id
          : `msg:${next.transcript[0].id}`,
      );
    });
  });

  describe("assistant_delta", () => {
    it("creates assistant message on first delta", () => {
      const state = makeState();
      const event: TuiEvent = { type: "assistant_delta", delta: "Hello " };
      const next = tuiReducer(state, event);
      expect(next.transcript).toHaveLength(1);
      expect(next.transcript[0].role).toBe("assistant");
      expect(next.transcript[0].text).toBe("Hello ");
      expect(next.transcript[0].isStreaming).toBe(true);
      expect(next.stream.assistantText).toBe("Hello ");
    });

    it("appends to existing assistant message", () => {
      const state = makeState();
      const delta1: TuiEvent = { type: "assistant_delta", delta: "Hello " };
      const after1 = tuiReducer(state, delta1);
      const delta2: TuiEvent = { type: "assistant_delta", delta: "world!" };
      const after2 = tuiReducer(after1, delta2);
      expect(after2.transcript).toHaveLength(1);
      expect(after2.transcript[0].text).toBe("Hello world!");
      expect(after2.stream.assistantText).toBe("Hello world!");
    });

    it("updates timeline streaming item", () => {
      const state = makeState();
      const event: TuiEvent = { type: "assistant_delta", delta: "Hello" };
      const next = tuiReducer(state, event);
      expect(next.timeline.items).toHaveLength(1);
      expect(next.timeline.items[0].text).toBe("Hello");
      expect(next.timeline.items[0].isStreaming).toBe(true);
    });
  });

  describe("thinking_delta", () => {
    it("sets thinkingActive", () => {
      const state = makeState();
      const event: TuiEvent = { type: "thinking_delta", delta: "Hmm" };
      const next = tuiReducer(state, event);
      expect(next.stream.thinkingActive).toBe(true);
    });
  });

  describe("message lifecycle", () => {
    it("message_update stores ordered assistant content blocks", () => {
      const state = tuiReducer(makeState(), { type: "stream_started" });

      const next = tuiReducer(state, {
        type: "message_update",
        message: {
          id: "assistant-step_1",
          role: "assistant",
          isStreaming: true,
          content: [
            { type: "thinking", thinking: "plan" },
            { type: "text", text: "answer" },
            { type: "thinking", thinking: "check" },
          ],
        },
        assistantEvent: { type: "text_delta", contentIndex: 1, delta: "answer" },
      });

      const item = next.timeline.items.find((i) => i.id === "msg:assistant-step_1");
      expect(item?.content?.map((block) => block.type)).toEqual(["thinking", "text", "thinking"]);
      expect(item?.text).toBe("answer");
      expect(item?.thinkingText).toBe("plancheck");
      expect(next.transcript[0].content?.map((block) => block.type)).toEqual([
        "thinking",
        "text",
        "thinking",
      ]);
    });

    it("message_update updates the same streaming item by message id", () => {
      const first = tuiReducer(makeState(), {
        type: "message_update",
        message: {
          id: "assistant-step_1",
          role: "assistant",
          isStreaming: true,
          content: [{ type: "text", text: "Hel" }],
        },
        assistantEvent: { type: "text_delta", contentIndex: 0, delta: "Hel" },
      });

      const second = tuiReducer(first, {
        type: "message_update",
        message: {
          id: "assistant-step_1",
          role: "assistant",
          isStreaming: true,
          content: [{ type: "text", text: "Hello" }],
        },
        assistantEvent: { type: "text_delta", contentIndex: 0, delta: "lo" },
      });

      expect(second.timeline.items).toHaveLength(1);
      expect(second.timeline.items[0].content?.[0]).toEqual({ type: "text", text: "Hello" });
      expect(second.transcript).toHaveLength(1);
      expect(second.transcript[0].text).toBe("Hello");
      expect(second.projection.orderedIds).toEqual(["msg:assistant-step_1"]);
    });

    it("orders out-of-order message lifecycle events by messageIndex within a run", () => {
      const later = tuiReducer(makeState(), {
        type: "message_update",
        runId: "run-1",
        messageIndex: 1,
        message: {
          id: "assistant-later",
          role: "assistant",
          isStreaming: true,
          content: [{ type: "text", text: "later" }],
        },
      });
      const earlier = tuiReducer(later, {
        type: "message_end",
        runId: "run-1",
        messageIndex: 0,
        message: {
          id: "assistant-earlier",
          role: "assistant",
          isStreaming: false,
          content: [{ type: "text", text: "earlier" }],
        },
      });

      expect(earlier.projection.orderedIds).toEqual([
        "msg:assistant-earlier",
        "msg:assistant-later",
      ]);
    });
  });

  describe("tool_call_started", () => {
    it("adds tool message with running status", () => {
      const state = makeState();
      const event: TuiEvent = {
        type: "tool_call_started",
        id: "tool-1",
        name: "read",
        args: { path: "/foo" },
      };
      const next = tuiReducer(state, event);
      expect(next.transcript).toHaveLength(1);
      expect(next.transcript[0].role).toBe("tool");
      expect(next.transcript[0].toolBlock?.name).toBe("read");
      expect(next.transcript[0].toolBlock?.status).toBe("running");
      expect(next.stream.currentToolCallId).toBe("tool-1");
    });
  });

  describe("tool_call_ended", () => {
    it("updates tool message to success", () => {
      const state = makeState();
      const start: TuiEvent = {
        type: "tool_call_started",
        id: "tool-1",
        name: "read",
        args: {},
      };
      const mid = tuiReducer(state, start);
      const end: TuiEvent = {
        type: "tool_call_ended",
        id: "tool-1",
        name: "read",
        result: "file content",
        isError: false,
      };
      const next = tuiReducer(mid, end);
      expect(next.transcript[0].toolBlock?.status).toBe("success");
      expect(next.transcript[0].toolBlock?.result).toBe("file content");
      expect(next.timeline.collapsedToolCallIds.has("tool-1")).toBe(true);
    });

    it("updates tool message to error", () => {
      const state = makeState();
      const start: TuiEvent = {
        type: "tool_call_started",
        id: "tool-1",
        name: "read",
        args: {},
      };
      const mid = tuiReducer(state, start);
      const end: TuiEvent = {
        type: "tool_call_ended",
        id: "tool-1",
        name: "read",
        result: "ENOENT",
        isError: true,
      };
      const next = tuiReducer(mid, end);
      expect(next.transcript[0].toolBlock?.status).toBe("error");
    });

    it("creates a completed tool item when tool_start was not observed", () => {
      const next = tuiReducer(makeState(), {
        type: "tool_call_ended",
        id: "tool-without-start",
        name: "read",
        result: "contents",
        isError: false,
      });

      expect(next.projection.orderedIds).toEqual(["tool:tool-without-start"]);
      expect(next.projection.itemsById["tool:tool-without-start"].toolStatus).toBe("success");
      expect(next.timeline.items).toEqual([next.projection.itemsById["tool:tool-without-start"]]);
    });

    it("renders an orphan tool immediately and re-parents it when any message event arrives", () => {
      const orphan = tuiReducer(makeState(), {
        type: "tool_call_started",
        id: "tool-1",
        name: "read",
        args: {},
        parentMessageId: "assistant-1",
        toolCallIndex: 0,
      });
      expect(orphan.projection.orderedIds).toEqual(["tool:tool-1"]);

      const reparented = tuiReducer(orphan, {
        type: "message_update",
        runId: "run-1",
        messageIndex: 0,
        message: {
          id: "assistant-1",
          role: "assistant",
          isStreaming: true,
          content: [{ type: "toolCall", id: "tool-1", name: "read", arguments: {} }],
        },
      });
      expect(reparented.projection.orderedIds).toEqual(["msg:assistant-1", "tool:tool-1"]);
      expect(reparented.timeline.items.map((item) => item.id)).toEqual(
        reparented.projection.orderedIds,
      );
    });

    it("keeps tools distinct when separate runs reuse the same provider call ID", () => {
      let state = makeState();
      for (const [runId, assistantId] of [
        ["run-1", "assistant-1"],
        ["run-2", "assistant-2"],
      ] as const) {
        state = tuiReducer(state, {
          type: "message_end",
          runId,
          messageIndex: 0,
          message: { id: assistantId, role: "assistant", isStreaming: false, content: [] },
        });
        state = tuiReducer(state, {
          type: "tool_call_started",
          runId,
          entityId: `${assistantId}:tool:0`,
          id: "provider-reused-id",
          name: "read",
          args: {},
          parentMessageId: assistantId,
          toolCallIndex: 0,
        });
      }

      expect(state.projection.orderedIds).toEqual([
        "msg:assistant-1",
        "tool:assistant-1:tool:0",
        "msg:assistant-2",
        "tool:assistant-2:tool:0",
      ]);
      expect(state.transcript.filter((message) => message.role === "tool")).toHaveLength(2);
    });
  });

  describe("approval projection", () => {
    it("creates a visible pending tool when approval arrives through the side channel", () => {
      const pending = tuiReducer(makeState(), {
        type: "approval_needed",
        callId: "approval-tool",
        toolName: "bash",
        toolArgs: { command: "git status" },
      });

      expect(pending.approval.pending?.callId).toBe("approval-tool");
      expect(pending.projection.orderedIds).toContain("tool:approval-tool");
      expect(pending.projection.itemsById["tool:approval-tool"].toolStatus).toBe("pending");
      expect(pending.timeline.items.map((item) => item.id)).toEqual(pending.projection.orderedIds);

      const started = tuiReducer(pending, {
        type: "tool_call_started",
        id: "approval-tool",
        name: "bash",
        args: { command: "git status" },
        parentMessageId: "assistant-1",
      });
      const parentArrived = tuiReducer(started, {
        type: "message_end",
        runId: "run-1",
        messageIndex: 0,
        message: {
          id: "assistant-1",
          role: "assistant",
          isStreaming: false,
          content: [],
        },
      });
      expect(parentArrived.projection.orderedIds).toEqual([
        "msg:assistant-1",
        "tool:approval-tool",
      ]);
    });
  });

  describe("turn_finished", () => {
    it("resets stream state", () => {
      const state = makeState();
      const running = tuiReducer(state, { type: "stream_started" });
      const finished: TuiEvent = {
        type: "turn_finished",
        status: "ok",
        transcript: [],
      };
      const next = tuiReducer(running, finished);
      expect(next.stream.status).toBe("idle");
      expect(next.stream.thinkingActive).toBe(false);
    });

    it("rebuilds from the durable ID-bearing entry snapshot", () => {
      const running = tuiReducer(makeState(), { type: "stream_started" });
      const next = tuiReducer(running, {
        type: "turn_finished",
        status: "completed",
        transcript: [{ role: "assistant", content: "position must not matter" }] as any,
        entries: [
          {
            type: "compaction",
            id: "compact-1",
            parentId: null,
            timestamp: "2024-01-01T00:00:00Z",
            summary: "stable summary",
            firstKeptEntryId: "assistant-1",
            tokensBefore: 900,
          },
          {
            type: "message",
            id: "assistant-1",
            parentId: "compact-1",
            timestamp: "2024-01-01T00:00:01Z",
            message: {
              role: "assistant",
              content: [{ type: "text", text: "stable answer" }],
            } as any,
          },
        ],
      });

      expect(next.projection.orderedIds).toEqual(["msg:compact-1", "msg:assistant-1"]);
      expect(next.projection.itemsById["msg:compact-1"].kind).toBe("compaction-summary");
      expect(next.projection.itemsById["msg:compact-1"].tokensBefore).toBe(900);
      expect(next.projection.itemsById["msg:assistant-1"].text).toBe("stable answer");
    });
  });

  describe("turn_failed", () => {
    it("adds error message to transcript AND timeline", () => {
      const state = makeState();
      const event: TuiEvent = { type: "turn_failed", error: "Network error" };
      const next = tuiReducer(state, event);

      // Transcript updated
      expect(next.transcript).toHaveLength(1);
      expect(next.transcript[0].text).toContain("Network error");
      expect(next.transcript[0].role).toBe("assistant");

      // Timeline synced — error item is visible
      expect(next.timeline.items).toHaveLength(1);
      expect(next.timeline.items[0].text).toContain("Network error");
      expect(next.timeline.items[0].kind).toBe("assistant-message");

      // Stream reset
      expect(next.stream.status).toBe("idle");
    });

    it("increments pendingNewItems when anchor is manual", () => {
      const state = makeState();
      state.timeline.anchor = "manual";
      const event: TuiEvent = { type: "turn_failed", error: "boom" };
      const next = tuiReducer(state, event);
      expect(next.timeline.pendingNewItems).toBe(1);
    });
  });

  describe("model_changed", () => {
    it("updates model state", () => {
      const state = makeState();
      const newModel: Model<string> = {
        id: "new-model",
        provider: "new-provider",
        label: "New",
      } as any;
      const newConfig: ModelProviderConfig = {
        provider: "new-provider",
        auth: { type: "api_key", key: "new-key" },
      } as any;
      const event: TuiEvent = {
        type: "model_changed",
        model: newModel,
        providerConfig: newConfig,
      };
      const next = tuiReducer(state, event);
      expect(next.model.current.id).toBe("new-model");
      expect(next.model.current.provider).toBe("new-provider");
    });
  });

  describe("layout_resized", () => {
    it("updates viewport dimensions", () => {
      const state = makeState();
      const event: TuiEvent = { type: "layout_resized", width: 120, height: 40 };
      const next = tuiReducer(state, event);
      expect(next.layout.viewport.width).toBe(120);
      expect(next.layout.viewport.height).toBe(40);
    });
  });

  describe("usage_updated", () => {
    it("accumulates usage", () => {
      const state = makeState();
      const event: TuiEvent = {
        type: "usage_updated",
        inputTokens: 100,
        outputTokens: 50,
        totalCost: 0.01,
      };
      const next = tuiReducer(state, event);
      expect(next.usage.inputTokens).toBe(100);
      expect(next.usage.outputTokens).toBe(50);
      expect(next.usage.totalCost).toBe(0.01);
    });

    it("preserves existing values when not provided", () => {
      const state = makeState();
      const event1: TuiEvent = {
        type: "usage_updated",
        inputTokens: 100,
      };
      const next1 = tuiReducer(state, event1);
      const event2: TuiEvent = {
        type: "usage_updated",
        outputTokens: 50,
      };
      const next2 = tuiReducer(next1, event2);
      expect(next2.usage.inputTokens).toBe(100);
      expect(next2.usage.outputTokens).toBe(50);
    });
  });

  describe("selectStatus", () => {
    it("includes the latest unexpired notification while idle", () => {
      const state = makeState();
      const next = tuiReducer(state, {
        type: "notification_added",
        notification: {
          id: "notif-1",
          severity: "success",
          source: "ui",
          message: "Saved",
          createdAt: Date.now(),
        },
      });

      expect(selectStatus(next)).toMatchObject({
        state: "idle",
        notification: { severity: "success", message: "Saved" },
      });
    });

    it("keeps working state ahead of notifications", () => {
      const state = makeState();
      const notified = tuiReducer(state, {
        type: "notification_added",
        notification: {
          id: "notif-1",
          severity: "info",
          source: "ui",
          message: "Queued",
          createdAt: Date.now(),
        },
      });
      const running = tuiReducer(notified, { type: "user_submitted", text: "hello" });

      expect(selectStatus(running)).toEqual({ state: "working" });
    });

    it("omits expired notifications from the status contract", () => {
      const now = Date.now();
      const state = makeState();
      const next = tuiReducer(state, {
        type: "notification_added",
        notification: {
          id: "notif-1",
          severity: "warning",
          source: "ui",
          message: "Expired",
          createdAt: now - 2_000,
          ttlMs: 1_000,
        },
      });

      expect(selectStatus(next, now)).toEqual({ state: "idle" });
    });
  });

  describe("session_resumed", () => {
    it("loads transcript and updates session", () => {
      const state = makeState();
      const event: TuiEvent = {
        type: "session_resumed",
        sessionId: "sess-1",
        sessionName: "My Session",
        transcript: [
          {
            id: "msg-1",
            role: "user",
            text: "Hello",
          },
        ],
      };
      const next = tuiReducer(state, event);
      expect(next.session.sessionId).toBe("sess-1");
      expect(next.session.sessionName).toBe("My Session");
      expect(next.transcript).toHaveLength(1);
      expect(next.session.messageCount).toBe(1);
    });

    it("rebuilds the same semantic timeline produced by a completed live turn", () => {
      let live = tuiReducer(makeState(), { type: "user_submitted", text: "Check" });
      const assistant = {
        id: "assistant-run-1-step_1",
        role: "assistant" as const,
        isStreaming: false,
        content: [
          { type: "toolCall" as const, id: "tool-1", name: "read", arguments: { path: "x" } },
        ],
      };
      live = tuiReducer(live, {
        type: "message_end",
        runId: "run-1",
        eventSeq: 1,
        messageIndex: 0,
        message: assistant,
      });
      live = tuiReducer(live, {
        type: "tool_call_ended",
        runId: "run-1",
        eventSeq: 2,
        id: "tool-1",
        name: "read",
        result: "contents",
        isError: false,
        parentMessageId: assistant.id,
        toolCallIndex: 0,
      });
      live = tuiReducer(live, {
        type: "turn_finished",
        status: "completed",
        transcript: [
          { role: "user", content: "Check" },
          assistant,
          {
            role: "toolResult",
            toolCallId: "tool-1",
            toolName: "read",
            content: "contents",
            details: "contents",
            isError: false,
          },
        ] as any,
      });

      const resumed = tuiReducer(makeState(), {
        type: "session_resumed",
        sessionId: "session-1",
        transcript: live.transcript,
      });
      const semantic = (state: ReturnType<typeof makeState>) =>
        state.projection.orderedIds.map((id) => {
          const item = state.projection.itemsById[id];
          return [item.kind, item.messageId, item.toolCallId, item.toolStatus];
        });

      expect(semantic(resumed)).toEqual(semantic(live));
    });
  });

  describe("message ID uniqueness", () => {
    it("assigns unique, sequential IDs across multiple messages", () => {
      const state = makeState();
      const a = tuiReducer(state, { type: "user_submitted", text: "msg1" });
      const b = tuiReducer(a, { type: "user_submitted", text: "msg2" });
      const c = tuiReducer(b, { type: "user_submitted", text: "msg3" });

      const ids = c.transcript.map((m) => m.id);
      const unique = new Set(ids);
      expect(unique.size).toBe(3);
      expect(ids[0]).not.toBe(ids[1]);
      expect(ids[1]).not.toBe(ids[2]);
    });
  });

  describe("tree navigation", () => {
    it("handles tree_navigation_started by transitioning to running state", () => {
      const state = makeState();
      const next = tuiReducer(state, {
        type: "tree_navigation_started",
        operationId: "op-123",
        entryId: "msg-4",
      });
      expect(next.session.navigation).toEqual({
        status: "running",
        operationId: "op-123",
        entryId: "msg-4",
      });
    });

    it("handles tree_navigation_succeeded by updating state if operation matches", () => {
      const state = makeState();
      const started = tuiReducer(state, {
        type: "tree_navigation_started",
        operationId: "op-123",
        entryId: "msg-4",
      });

      const next = tuiReducer(started, {
        type: "tree_navigation_succeeded",
        operationId: "op-123",
        result: {
          status: "navigated",
          sessionId: "session-abc",
          oldLeafId: "leaf-1",
          newLeafId: "leaf-2",
          selectedEntryId: "msg-4",
          transcript: [
            { id: "msg-1", role: "user", text: "hello" },
            { id: "msg-2", role: "assistant", text: "hi" },
          ],
          editorDraft: {
            text: "restored user text",
            revision: 5,
            source: { kind: "session_tree", sessionId: "session-abc", entryId: "msg-4" },
          },
          surfaceId: "surf-1",
        },
      });

      expect(next.session.navigation.status).toBe("idle");
      expect(next.session.sessionId).toBe("session-abc");
      expect(next.session.messageCount).toBe(2);
      expect(next.transcript).toHaveLength(2);
      expect(next.transcript[0].text).toBe("hello");
      expect(next.timeline.items).toHaveLength(2);
      expect(next.input.draft).toBe("restored user text");
      expect(next.input.revision).toBe(5);
    });

    it("ignores tree_navigation_succeeded if operation does not match (stale)", () => {
      const state = makeState();
      const started = tuiReducer(state, {
        type: "tree_navigation_started",
        operationId: "op-123",
        entryId: "msg-4",
      });

      const next = tuiReducer(started, {
        type: "tree_navigation_succeeded",
        operationId: "op-different",
        result: {
          status: "navigated",
          sessionId: "session-abc",
          oldLeafId: "leaf-1",
          newLeafId: "leaf-2",
          selectedEntryId: "msg-4",
          transcript: [{ id: "msg-1", role: "user", text: "hello" }],
          surfaceId: "surf-1",
        },
      });

      // State remains unchanged
      expect(next).toBe(started);
    });

    it("handles tree_navigation_failed by setting status and storing error if operation matches", () => {
      const state = makeState();
      const started = tuiReducer(state, {
        type: "tree_navigation_started",
        operationId: "op-123",
        entryId: "msg-4",
      });

      const next = tuiReducer(started, {
        type: "tree_navigation_failed",
        operationId: "op-123",
        error: "Some navigation error occurred",
      });

      expect(next.session.navigation).toEqual({
        status: "failed",
        error: "Some navigation error occurred",
        operationId: "op-123",
        entryId: "msg-4",
      });
    });

    it("ignores tree_navigation_failed if operation does not match (stale)", () => {
      const state = makeState();
      const started = tuiReducer(state, {
        type: "tree_navigation_started",
        operationId: "op-123",
        entryId: "msg-4",
      });

      const next = tuiReducer(started, {
        type: "tree_navigation_failed",
        operationId: "op-different",
        error: "Some other navigation error occurred",
      });

      expect(next).toBe(started);
    });

    it("invalidates running navigation when session is resumed", () => {
      const state = makeState();
      const started = tuiReducer(state, {
        type: "tree_navigation_started",
        operationId: "op-123",
        entryId: "msg-4",
      });

      const resumed = tuiReducer(started, {
        type: "session_resumed",
        sessionId: "sess-2",
        sessionName: "Another Session",
        transcript: [],
      });

      expect(resumed.session.navigation).toEqual({
        status: "idle",
      });
    });
  });
});
