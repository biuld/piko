// ============================================================================
// Layout policy unit tests
// ============================================================================

import { describe, expect, it } from "bun:test";
import { measureTextLines, truncateToWidth, visibleWidth } from "../src/layout/measure.js";
import {
  applyLayoutPolicies,
  detectBottomBarDensity,
  detectLayoutMode,
  getBottomBarRows,
  getEditorMaxRows,
} from "../src/layout/policies.js";
import type { Model, ModelProviderConfig } from "../src/shared/index.js";
import { createDefaultTuiState } from "../src/state/state.js";

function makeState() {
  const model: Model<string> = { id: "test", provider: "test", label: "Test" } as any;
  const config: ModelProviderConfig = {
    provider: "test",
    auth: { type: "api_key", key: "k" },
  } as any;
  return createDefaultTuiState(model, config, "/cwd");
}

// ============================================================================
// detectLayoutMode
// ============================================================================
describe("detectLayoutMode", () => {
  it("returns regular for large terminals", () => {
    expect(detectLayoutMode(120, 40)).toBe("regular");
    expect(detectLayoutMode(100, 24)).toBe("regular");
  });

  it("returns compact for medium terminals", () => {
    expect(detectLayoutMode(80, 20)).toBe("compact");
    expect(detectLayoutMode(60, 16)).toBe("compact");
    expect(detectLayoutMode(99, 23)).toBe("compact"); // width < 100
  });

  it("returns minimal for small terminals", () => {
    expect(detectLayoutMode(50, 15)).toBe("minimal");
    expect(detectLayoutMode(80, 10)).toBe("minimal"); // height < 16
  });
});

// ============================================================================
// detectBottomBarDensity
// ============================================================================
describe("detectBottomBarDensity", () => {
  it("returns full for wide terminals", () => {
    expect(detectBottomBarDensity(120)).toBe("full");
    expect(detectBottomBarDensity(200)).toBe("full");
  });

  it("returns compact for medium terminals", () => {
    expect(detectBottomBarDensity(80)).toBe("compact");
    expect(detectBottomBarDensity(100)).toBe("compact");
  });

  it("returns minimal for narrow terminals", () => {
    expect(detectBottomBarDensity(60)).toBe("minimal");
    expect(detectBottomBarDensity(40)).toBe("minimal");
  });
});

// ============================================================================
// visibleWidth
// ============================================================================
describe("visibleWidth", () => {
  it("returns string length for plain text", () => {
    expect(visibleWidth("hello")).toBe(5);
  });

  it("strips ANSI codes", () => {
    const colored = "\x1b[31mhello\x1b[0m";
    expect(visibleWidth(colored)).toBe(5);
  });

  it("handles complex ANSI", () => {
    const text = "\x1b[38;2;255;0;0mred\x1b[39m";
    expect(visibleWidth(text)).toBe(3);
  });

  it("counts CJK characters as double-width terminal cells", () => {
    expect(visibleWidth("当前partial")).toBe(11);
  });
});

// ============================================================================
// truncateToWidth
// ============================================================================
describe("truncateToWidth", () => {
  it("returns text if it fits", () => {
    expect(truncateToWidth("hello", 10)).toBe("hello");
  });

  it("truncates long text", () => {
    const result = truncateToWidth("hello world", 5);
    expect(visibleWidth(result)).toBeLessThanOrEqual(5);
  });

  it("handles empty string", () => {
    expect(truncateToWidth("", 10)).toBe("");
  });

  it("preserves ANSI codes at truncation boundary", () => {
    const colored = "\x1b[31mlong text here\x1b[0m";
    const result = truncateToWidth(colored, 4);
    expect(result).toContain("\x1b[31m");
  });

  it("truncates CJK text by terminal cell width", () => {
    const result = truncateToWidth("当前partial", 5, "…");
    expect(result).toBe("当前…");
    expect(visibleWidth(result)).toBeLessThanOrEqual(5);
  });
});

// ============================================================================
// measureTextLines
// ============================================================================
describe("measureTextLines", () => {
  it("counts lines for single line text", () => {
    expect(measureTextLines("hello", 10)).toBe(1);
  });

  it("counts wrapped lines", () => {
    expect(measureTextLines("hello world", 5)).toBe(3); // "hello", " worl", "d"
  });

  it("counts explicit newlines", () => {
    expect(measureTextLines("a\nb\nc", 10)).toBe(3);
  });

  it("handles empty text", () => {
    expect(measureTextLines("", 10)).toBe(0);
  });
});

// ============================================================================
// applyLayoutPolicies
// ============================================================================
describe("applyLayoutPolicies", () => {
  it("updates mode and density from viewport", () => {
    const state = makeState();
    state.layout.viewport = { width: 60, height: 24 };

    const result = applyLayoutPolicies(state);
    expect(result.layout.mode).toBe("compact");
    expect(result.layout.bottomBar.density).toBe("minimal");
  });

  it("preserves activeRegion when set", () => {
    const state = makeState();
    state.layout.activeRegion = "chat";

    const result = applyLayoutPolicies(state);
    expect(result.layout.activeRegion).toBe("chat");
  });

  it("preserves existing viewport dimensions", () => {
    const state = makeState();
    state.layout.viewport = { width: 100, height: 30 };

    const result = applyLayoutPolicies(state);
    expect(result.layout.viewport.width).toBe(100);
    expect(result.layout.viewport.height).toBe(30);
  });
});

// ============================================================================
// getEditorMaxRows / getBottomBarRows
// ============================================================================
describe("getEditorMaxRows", () => {
  it("returns 10 for regular", () => expect(getEditorMaxRows("regular")).toBe(10));
  it("returns 5 for compact", () => expect(getEditorMaxRows("compact")).toBe(5));
  it("returns 3 for minimal", () => expect(getEditorMaxRows("minimal")).toBe(3));
});

describe("getBottomBarRows", () => {
  it("returns 4 for regular", () => expect(getBottomBarRows("regular")).toBe(4));
  it("returns 2 for compact", () => expect(getBottomBarRows("compact")).toBe(2));
  it("returns 1 for minimal", () => expect(getBottomBarRows("minimal")).toBe(1));
});
