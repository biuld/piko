// ============================================================================
// TUI Reducer unit tests
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-engine-protocol";
import { describe, expect, it } from "vitest";
import type { TuiEvent } from "../src/state/events.js";
import { tuiReducer } from "../src/state/reducer.js";
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
  describe("user_input_changed", () => {
    it("updates input text", () => {
      const state = makeState();
      const event: TuiEvent = { type: "user_input_changed", text: "hello" };
      const next = tuiReducer(state, event);
      expect(next.input.text).toBe("hello");
    });
  });

  describe("user_submitted", () => {
    it("clears input and adds user message", () => {
      const state = { ...makeState(), input: { text: "hello", focused: true } };
      const event: TuiEvent = { type: "user_submitted", text: "hello" };
      const next = tuiReducer(state, event);
      expect(next.input.text).toBe("");
      expect(next.transcript).toHaveLength(1);
      expect(next.transcript[0].role).toBe("user");
      expect(next.transcript[0].text).toBe("hello");
      expect(next.stream.status).toBe("running");
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

    it("scrolls to bottom by default", () => {
      const state = makeState();
      const event: TuiEvent = { type: "assistant_delta", delta: "x" };
      const next = tuiReducer(state, event);
      expect(next.layout.chat.scrollAnchor).toBe("bottom");
    });

    it("preserves manual scroll anchor", () => {
      const state = makeState();
      state.layout.chat.scrollAnchor = "manual";
      const event: TuiEvent = { type: "assistant_delta", delta: "x" };
      const next = tuiReducer(state, event);
      expect(next.layout.chat.scrollAnchor).toBe("manual");
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
      expect(next.stream.currentToolName).toBe("read");
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
      expect(next.stream.currentToolCallId).toBeUndefined();
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
    it("adds error message", () => {
      const state = makeState();
      const event: TuiEvent = { type: "turn_failed", error: "Network error" };
      const next = tuiReducer(state, event);
      expect(next.transcript).toHaveLength(1);
      expect(next.transcript[0].text).toContain("Network error");
      expect(next.stream.status).toBe("idle");
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

  describe("overlay opened/closed", () => {
    it("sets overlay state and active region on open", () => {
      const state = makeState();
      const event: TuiEvent = {
        type: "overlay_opened",
        overlay: { kind: "model", isOpen: true, placement: "modal" },
      };
      const next = tuiReducer(state, event);
      expect(next.overlay).toBeTruthy();
      expect(next.overlay?.kind).toBe("model");
      expect(next.layout.activeRegion).toBe("overlay");
    });

    it("clears overlay state on close", () => {
      const state = makeState();
      const opened = tuiReducer(state, {
        type: "overlay_opened",
        overlay: { kind: "model", isOpen: true, placement: "modal" },
      });
      const closed: TuiEvent = { type: "overlay_closed" };
      const next = tuiReducer(opened, closed);
      expect(next.overlay).toBeNull();
      expect(next.layout.activeRegion).toBe("editor");
    });
  });

  describe("tool_block_toggled", () => {
    it("adds and removes from collapsed set", () => {
      const state = makeState();
      const toggle1: TuiEvent = { type: "tool_block_toggled", toolCallId: "t1" };
      const next1 = tuiReducer(state, toggle1);
      expect(next1.layout.chat.collapsedToolCallIds.has("t1")).toBe(true);

      const toggle2: TuiEvent = { type: "tool_block_toggled", toolCallId: "t1" };
      const next2 = tuiReducer(next1, toggle2);
      expect(next2.layout.chat.collapsedToolCallIds.has("t1")).toBe(false);
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
});
