import { describe, expect, it } from "bun:test";
import { getVisibleAssistantBlocks } from "../src/renderer/opentui/timeline/assistant-blocks.js";
import type { TimelineItem } from "../src/timeline/types.js";

describe("AssistantMessageView helpers", () => {
  it("keeps text/thinking block order and drops toolCall blocks", () => {
    const item = {
      id: "msg:assistant-step_1",
      kind: "assistant-stream",
      content: [
        { type: "thinking", thinking: "plan" },
        { type: "toolCall", id: "tc1", name: "bash", arguments: {} },
        { type: "text", text: "answer" },
        { type: "thinking", thinking: "verify" },
      ],
    } satisfies TimelineItem;

    const blocks = getVisibleAssistantBlocks(item);

    expect(blocks.map((block) => block.type)).toEqual(["thinking", "text", "thinking"]);
  });

  it("returns an empty list for tool-only assistant content", () => {
    const item = {
      id: "msg:assistant-step_1",
      kind: "assistant-stream",
      content: [{ type: "toolCall", id: "tc1", name: "bash", arguments: {} }],
    } satisfies TimelineItem;

    expect(getVisibleAssistantBlocks(item)).toEqual([]);
  });
});
