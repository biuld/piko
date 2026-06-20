import { describe, expect, test } from "bun:test";
import { getDefaultTheme } from "../src/theme/resolve.js";

describe("ToolApprovalBody theme", () => {
  test("uses tokens present in the resolved theme", () => {
    const theme = getDefaultTheme();
    for (const token of ["text.warning", "text.dim", "text.primary", "text.muted"]) {
      expect(() => theme.color(token)).not.toThrow();
    }
  });
});
