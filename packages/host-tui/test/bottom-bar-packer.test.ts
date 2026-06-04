// ============================================================================
// BottomBarPacker unit tests
// ============================================================================

import { describe, expect, it } from "vitest";
import { middleTruncate, packBottomBar, visibleLength } from "../src/layout/bottom-bar-packer.js";

const sampleInput = {
  cwd: "~/projects/piko",
  gitBranch: "main",
  sessionName: "my-session",
  modelProvider: "anthropic",
  modelId: "claude-sonnet-4-5-20250929",
  thinkingLevel: "high",
  inputTokens: "12.4k",
  outputTokens: "3.1k",
  cacheTokens: "8.2k",
  cost: "$0.042",
  contextPercent: "42%",
  contextWindow: "200k",
  messageCount: 15,
  hints: ["^P model", "^T thinking", "^R resume"],
};

describe("packBottomBar", () => {
  it("fits everything at width 120", () => {
    const result = packBottomBar(sampleInput, 120);
    expect(result.line1).toContain("~/projects/piko");
    expect(result.line1).toContain("main");
    expect(result.line2).toContain("↑12.4k");
    expect(result.line2).toContain("$0.042");
    expect(result.truncated).toBe(false);
  });

  it("fits essentials at width 80", () => {
    const result = packBottomBar(sampleInput, 80);
    // Line should have something
    expect(result.line1.length).toBeGreaterThan(0);
    expect(result.line2.length).toBeGreaterThan(0);
  });

  it("handles minimal width 40", () => {
    const result = packBottomBar(sampleInput, 40);
    expect(result.line1.length).toBeGreaterThan(0);
    // Some fields will be dropped
  });

  it("marks truncated when fields overflow", () => {
    const result = packBottomBar(sampleInput, 30);
    expect(result.truncated).toBe(true);
  });

  it("handles no git branch or session", () => {
    const input = { ...sampleInput, gitBranch: undefined, sessionName: undefined };
    const result = packBottomBar(input, 80);
    expect(result.line1).not.toContain("main");
  });

  it("handles no hints", () => {
    const input = { ...sampleInput, hints: [] };
    const result = packBottomBar(input, 80);
    expect(result.line2).toBeDefined();
  });

  it("handles no cost", () => {
    const input = { ...sampleInput, cost: "" };
    const result = packBottomBar(input, 80);
    expect(result.line2).not.toContain("$");
  });
});

describe("visibleLength", () => {
  it("returns plain text length", () => {
    expect(visibleLength("hello")).toBe(5);
  });

  it("strips ANSI codes", () => {
    expect(visibleLength("\x1b[31mhello\x1b[0m")).toBe(5);
  });

  it("returns 0 for empty string", () => {
    expect(visibleLength("")).toBe(0);
  });
});

describe("middleTruncate", () => {
  it("returns text if it fits", () => {
    expect(middleTruncate("hello", 10)).toBe("hello");
  });

  it("middle-truncates long text", () => {
    const result = middleTruncate("/very/long/path/that/should/be/truncated", 20);
    expect(result).toContain("...");
    expect(visibleLength(result)).toBeLessThanOrEqual(20);
  });

  it("handles very narrow width", () => {
    const result = middleTruncate("long/path", 5);
    expect(visibleLength(result)).toBeLessThanOrEqual(5);
  });

  it("handles empty string", () => {
    expect(middleTruncate("", 10)).toBe("");
  });
});
