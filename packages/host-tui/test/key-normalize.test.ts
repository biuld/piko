// ============================================================================
// Key normalization tests
// ============================================================================

import { describe, expect, it } from "bun:test";
import { normalizeKeyEvent, normalizeKeyName } from "../src/focus/index.js";

describe("normalizeKeyName", () => {
  it("normalizes arrow and escape aliases", () => {
    expect(normalizeKeyName("ArrowUp")).toBe("up");
    expect(normalizeKeyName("arrow_down")).toBe("down");
    expect(normalizeKeyName("Enter")).toBe("return");
    expect(normalizeKeyName("esc")).toBe("escape");
    expect(normalizeKeyName(undefined, "\x1b")).toBe("escape");
  });
});

describe("normalizeKeyEvent", () => {
  it("derives printable char only for plain single-character sequences", () => {
    expect(normalizeKeyEvent({ name: "a", sequence: "a" })).toEqual({
      name: "a",
      ctrl: false,
      shift: false,
      alt: false,
      meta: false,
      char: "a",
    });

    expect(normalizeKeyEvent({ name: "a", sequence: "a", ctrl: true })?.char).toBeUndefined();
  });

  it("returns null when no key name can be derived", () => {
    expect(normalizeKeyEvent({})).toBeNull();
  });

  it("preserves an existing normalized char", () => {
    expect(normalizeKeyEvent({ name: "x", char: "x" })?.char).toBe("x");
  });
});
