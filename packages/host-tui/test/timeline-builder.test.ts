import { describe, expect, it } from "bun:test";
import { handleSessionResumed } from "../src/state/reducers/handleSession.js";
import type { TuiMessageViewModel } from "../src/state/state.js";
import { createDefaultTuiState } from "../src/state/state.js";
import { buildTimelineItem } from "../src/timeline/timeline-builder.js";

describe("buildTimelineItem", () => {
  it("wraps legacy bare tool transcript text as a tool result", () => {
    const item = buildTimelineItem({
      id: "legacy-tool",
      role: "tool",
      text: "done",
    });

    expect(item.kind).toBe("tool-result");
    expect(item.toolName).toBe("tool");
    expect(item.toolStatus).toBe("success");
    expect(item.toolResult).toBe("done");
  });
});

describe("handleSessionResumed", () => {
  it("collapses completed tool results by default", () => {
    const transcript: TuiMessageViewModel[] = [
      {
        id: "tool-msg",
        role: "tool",
        text: "",
        toolBlock: {
          toolCallId: "tc-1",
          name: "bash",
          args: { command: "npm test" },
          status: "success",
          result: "\n\nok\n\n",
          isCollapsed: false,
        },
      },
    ];
    const state = createDefaultTuiState(
      { id: "m", provider: "p", label: "m" },
      { provider: "p", auth: { type: "api_key", key: "k" } },
      "/tmp",
    );

    const next = handleSessionResumed(state, {
      type: "session_resumed",
      sessionId: "s",
      transcript,
    });

    expect(next.timeline.collapsedToolCallIds.has("tc-1")).toBe(true);
  });
});
