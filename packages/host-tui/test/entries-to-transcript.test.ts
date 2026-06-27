import { describe, expect, it } from "bun:test";
import type { SessionTreeEntry } from "../src/shared/session/session-types.js";
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

  it("preserves structural summary entry identity and compaction metadata", () => {
    const entries: SessionTreeEntry[] = [
      {
        type: "branch_summary",
        id: "branch-1",
        parentId: null,
        timestamp: "2024-01-01T00:00:00Z",
        fromId: "old-leaf",
        summary: "Previous branch",
      },
      {
        type: "compaction",
        id: "compact-1",
        parentId: "branch-1",
        timestamp: "2024-01-01T00:00:01Z",
        firstKeptEntryId: "branch-1",
        summary: "Earlier context",
        tokensBefore: 1234,
      },
    ];

    expect(entriesToTranscript(entries)).toEqual([
      { id: "branch-1", role: "branchSummary", text: "Previous branch" },
      {
        id: "compact-1",
        role: "compactionSummary",
        text: "Earlier context",
        tokensBefore: 1234,
      },
    ]);
  });

  it("namespaces reused provider tool call IDs by durable assistant entry", () => {
    const assistant = (id: string, parentId: string | null): SessionTreeEntry => ({
      type: "message",
      id,
      parentId,
      timestamp: "2024-01-01T00:00:00Z",
      message: {
        role: "assistant",
        content: [{ type: "toolCall", id: "reused", name: "read", arguments: {} }],
      } as any,
    });

    const transcript = entriesToTranscript([
      assistant("assistant-1", null),
      assistant("assistant-2", "assistant-1"),
    ]);

    expect(transcript.map((message) => message.toolBlock?.toolEntityId)).toEqual([
      "assistant-1:reused",
      "assistant-2:reused",
    ]);
  });
});
