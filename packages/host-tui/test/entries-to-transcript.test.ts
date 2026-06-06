import { describe, expect, it } from "bun:test";
import type { SessionTreeEntry } from "piko-session";
import { entriesToTranscript } from "../src/timeline/entries-to-transcript.js";

describe("entriesToTranscript", () => {
  it("reconstructs tool blocks from assistant tool calls and tool results", () => {
    const entries: SessionTreeEntry[] = [
      {
        type: "message",
        id: "a1",
        parentId: null,
        timestamp: "2024-01-01T00:00:00Z",
        message: {
          role: "assistant",
          content: [
            { type: "text", text: "Let me inspect that." },
            { type: "toolCall", id: "tc-1", name: "read", arguments: { path: "README.md" } },
          ],
        } as any,
      },
      {
        type: "message",
        id: "t1",
        parentId: "a1",
        timestamp: "2024-01-01T00:00:01Z",
        message: {
          role: "toolResult",
          toolCallId: "tc-1",
          toolName: "read",
          content: [{ type: "text", text: "README contents" }],
          details: "README contents",
          isError: false,
        } as any,
      },
    ];

    const transcript = entriesToTranscript(entries);
    const tool = transcript.find((message) => message.role === "tool");

    expect(tool?.text).toBe("");
    expect(tool?.toolBlock?.toolCallId).toBe("tc-1");
    expect(tool?.toolBlock?.name).toBe("read");
    expect(tool?.toolBlock?.args).toEqual({ path: "README.md" });
    expect(tool?.toolBlock?.status).toBe("success");
    expect(tool?.toolBlock?.result).toBe("README contents");
  });

  it("does not create a blank assistant transcript item for tool-only assistant messages", () => {
    const entries: SessionTreeEntry[] = [
      {
        type: "message",
        id: "a1",
        parentId: null,
        timestamp: "2024-01-01T00:00:00Z",
        message: {
          role: "assistant",
          content: [{ type: "toolCall", id: "tc-1", name: "bash", arguments: { command: "pwd" } }],
        } as any,
      },
    ];

    const transcript = entriesToTranscript(entries);

    expect(transcript).toHaveLength(1);
    expect(transcript[0].role).toBe("tool");
    expect(transcript[0].toolBlock?.name).toBe("bash");
  });
});
