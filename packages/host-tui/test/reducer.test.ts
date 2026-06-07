// ============================================================================
// TUI Reducer unit tests
// ============================================================================

import { describe, expect, it } from "bun:test";
import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import type { TuiEvent } from "../src/state/events.js";
import { tuiReducer } from "../src/state/reducers/index.js";
import { selectStatus } from "../src/state/selectors.js";
import { createDefaultTuiState } from "../src/state/state.js";

function makeState() {
  const model: Model<string> = { id: "test-model", provider: "test-provider", label: "Test Model" };
  const providerConfig: EngineProviderConfig = {
    provider: "test-provider",
    auth: { type: "api_key", key: "test-key" },
  };
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
      };
      const newConfig: EngineProviderConfig = {
        provider: "new-provider",
        auth: { type: "api_key", key: "new-key" },
      };
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
});
