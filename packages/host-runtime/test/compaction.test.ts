import { describe, expect, it } from "vitest";
import {
  estimateContextTokens,
  estimateTokens,
  findCutPoint,
  findTurnStartIndex,
  prepareCompaction,
  shouldCompact,
} from "../src/compaction/index.js";
import type { CompactionSettings } from "../src/compaction/types.js";
import type { AgentMessage, SessionTreeEntry } from "../src/session/pi/types.js";

// ============================================================================
// Helpers to create fake session entries
// ============================================================================

let _entryId = 0;
function entryId(): string {
  return `entry-${++_entryId}`;
}

function userMsg(text: string): SessionTreeEntry {
  return {
    type: "message",
    id: entryId(),
    parentId: null,
    timestamp: new Date().toISOString(),
    message: { role: "user", content: text, timestamp: Date.now() },
  };
}

function assistantMsg(text: string, usage?: { input: number; output: number }): SessionTreeEntry {
  return {
    type: "message",
    id: entryId(),
    parentId: null,
    timestamp: new Date().toISOString(),
    message: {
      role: "assistant",
      content: [{ type: "text", text }],
      timestamp: Date.now(),
      stopReason: "end_turn",
      usage: usage
        ? {
            input: usage.input,
            output: usage.output,
            totalTokens: usage.input + usage.output,
            cacheRead: 0,
            cacheWrite: 0,
          }
        : undefined,
    } as any,
  };
}

function agentMsg(text: string): AgentMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    timestamp: Date.now(),
    stopReason: "end_turn",
  } as any;
}

// ============================================================================
// Tests
// ============================================================================

describe("estimateTokens", () => {
  it("estimates user messages", () => {
    const msg: AgentMessage = { role: "user", content: "hello world", timestamp: 0 };
    expect(estimateTokens(msg)).toBeGreaterThan(0);
    // "hello world" = 11 chars → ceil(11/4) = 3
    expect(estimateTokens(msg)).toBe(3);
  });

  it("estimates assistant messages", () => {
    const msg = agentMsg("hello world");
    expect(estimateTokens(msg)).toBeGreaterThan(0);
    // "hello world" = 11 chars → ceil(11/4) = 3
    expect(estimateTokens(msg)).toBe(3);
  });

  it("returns 0 for unknown message types", () => {
    const msg = { role: "unknown" } as any;
    expect(estimateTokens(msg)).toBe(0);
  });
});

describe("shouldCompact", () => {
  const settings: CompactionSettings = {
    enabled: true,
    reserveTokens: 1000,
    keepRecentTokens: 5000,
  };

  it("returns false when disabled", () => {
    expect(shouldCompact(90000, 100000, { ...settings, enabled: false })).toBe(false);
  });

  it("returns false below threshold", () => {
    // contextTokens=90000, contextWindow=100000, reserveTokens=1000
    // threshold = 100000 - 1000 = 99000
    // 90000 < 99000 → false
    expect(shouldCompact(90000, 100000, settings)).toBe(false);
  });

  it("returns true above threshold", () => {
    // 99500 > 99000 → true
    expect(shouldCompact(99500, 100000, settings)).toBe(true);
  });

  it("returns true at exactly threshold + 1", () => {
    expect(shouldCompact(99001, 100000, settings)).toBe(true);
  });
});

describe("estimateContextTokens", () => {
  it("estimates from messages without usage", () => {
    const msgs: AgentMessage[] = [
      { role: "user", content: "hello", timestamp: 0 },
      agentMsg("world"),
    ];
    const result = estimateContextTokens(msgs);
    expect(result.tokens).toBeGreaterThan(0);
    expect(result.lastUsageIndex).toBeNull();
  });

  it("uses provider usage when available", () => {
    const msgs: AgentMessage[] = [
      { role: "user", content: "hello", timestamp: 0 },
      {
        role: "assistant",
        content: [{ type: "text", text: "world" }],
        timestamp: 1,
        stopReason: "end_turn",
        usage: { input: 100, output: 50, totalTokens: 150, cacheRead: 0, cacheWrite: 0 },
      } as any,
      { role: "user", content: "extra", timestamp: 2 },
    ];
    const result = estimateContextTokens(msgs);
    expect(result.usageTokens).toBe(150);
    expect(result.trailingTokens).toBeGreaterThan(0);
    expect(result.tokens).toBeGreaterThan(150);
    expect(result.lastUsageIndex).toBe(1);
  });
});

describe("findTurnStartIndex", () => {
  it("finds user message start", () => {
    const entries = [
      {
        type: "message",
        id: "e1",
        parentId: null,
        timestamp: "",
        message: { role: "user", content: "hi", timestamp: 0 },
      },
      {
        type: "message",
        id: "e2",
        parentId: null,
        timestamp: "",
        message: { role: "assistant", content: [{ type: "text", text: "hello" }], timestamp: 0 },
      },
      {
        type: "message",
        id: "e3",
        parentId: null,
        timestamp: "",
        message: { role: "user", content: "more", timestamp: 0 },
      },
      {
        type: "message",
        id: "e4",
        parentId: null,
        timestamp: "",
        message: { role: "assistant", content: [{ type: "text", text: "ok" }], timestamp: 0 },
      },
    ] as SessionTreeEntry[];
    // From e4 (index 3), find the user turn start
    expect(findTurnStartIndex(entries, 3, 0)).toBe(2); // "more" user msg
    // From e2 (index 1), find the user turn start
    expect(findTurnStartIndex(entries, 1, 0)).toBe(0); // "hi" user msg
  });

  it("returns -1 when no user message found", () => {
    const entries = [
      {
        type: "message",
        id: "e1",
        parentId: null,
        timestamp: "",
        message: { role: "assistant", content: [{ type: "text", text: "ai" }], timestamp: 0 },
      },
    ] as SessionTreeEntry[];
    expect(findTurnStartIndex(entries, 0, 0)).toBe(-1);
  });
});

describe("findCutPoint", () => {
  function makeEntries(count: number): SessionTreeEntry[] {
    const entries: SessionTreeEntry[] = [];
    for (let i = 0; i < count; i++) {
      entries.push(i % 2 === 0 ? userMsg(`question ${i}`) : assistantMsg(`answer ${i}`));
    }
    return entries;
  }

  it("selects a valid cut point", () => {
    const entries = makeEntries(20);
    const result = findCutPoint(entries, 0, entries.length, 500);
    expect(result.firstKeptEntryIndex).toBeGreaterThanOrEqual(0);
    expect(result.firstKeptEntryIndex).toBeLessThan(entries.length);
  });

  it("returns start index when no valid cut points", () => {
    const entries = [
      {
        type: "compaction",
        id: "e1",
        parentId: null,
        timestamp: "",
        summary: "",
        firstKeptEntryId: "",
        tokensBefore: 0,
      } as SessionTreeEntry,
    ];
    const result = findCutPoint(entries, 0, entries.length, 100);
    expect(result.firstKeptEntryIndex).toBe(0);
    expect(result.turnStartIndex).toBe(-1);
    expect(result.isSplitTurn).toBe(false);
  });
});

describe("prepareCompaction", () => {
  const settings: CompactionSettings = {
    enabled: true,
    reserveTokens: 1000,
    keepRecentTokens: 500,
  };

  it("returns undefined for empty entries", () => {
    const result = prepareCompaction([], settings);
    expect(result.ok).toBe(true);
    expect(result.ok && result.value).toBeUndefined();
  });

  it("returns undefined when last entry is already compaction", () => {
    const entries: SessionTreeEntry[] = [
      userMsg("hello"),
      assistantMsg("world"),
      {
        type: "compaction",
        id: entryId(),
        parentId: null,
        timestamp: "",
        summary: "prev",
        firstKeptEntryId: "x",
        tokensBefore: 100,
      },
    ];
    const result = prepareCompaction(entries, settings);
    expect(result.ok).toBe(true);
    expect(result.ok && result.value).toBeUndefined();
  });

  it("prepares compaction for normal entries", () => {
    // Generate enough entries to exceed keepRecentTokens
    const entries: SessionTreeEntry[] = [];
    for (let i = 0; i < 20; i++) {
      entries.push(userMsg(`question ${i} with enough text to generate meaningful token counts`));
      entries.push(assistantMsg(`answer ${i} also with sufficient content for proper estimation`));
    }
    const compactSettings: CompactionSettings = {
      enabled: true,
      reserveTokens: 1000,
      keepRecentTokens: 50, // small budget forces cut
    };
    const result = prepareCompaction(entries, compactSettings);
    expect(result.ok).toBe(true);
    if (result.ok && result.value) {
      expect(result.value.firstKeptEntryId).toBeTruthy();
      expect(result.value.messagesToSummarize.length).toBeGreaterThan(0);
      expect(result.value.tokensBefore).toBeGreaterThan(0);
    }
  });

  it("errors on entries without UUIDs", () => {
    const entries = [
      {
        type: "message",
        id: "",
        parentId: null,
        timestamp: "",
        message: { role: "user", content: "hi", timestamp: 0 },
      } as SessionTreeEntry,
    ];
    const result = prepareCompaction(entries, { ...settings, keepRecentTokens: 0 });
    if (!result.ok) {
      expect(result.error.code).toBe("invalid_session");
    }
  });
});

describe("shouldCompact (boundary)", () => {
  const settings: CompactionSettings = {
    enabled: true,
    reserveTokens: 1000,
    keepRecentTokens: 5000,
  };

  it("handles large context window", () => {
    expect(shouldCompact(100_000, 200_000, settings)).toBe(false);
    expect(shouldCompact(199_001, 200_000, settings)).toBe(true);
  });

  it("handles disabled via settings", () => {
    expect(shouldCompact(999_999, 100_000, { ...settings, enabled: false })).toBe(false);
  });
});
