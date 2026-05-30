import type { Component } from "@earendil-works/pi-tui";
import { getTheme } from "../theme.js";

/**
 * Dynamic border component that adjusts to viewport width.
 * Renders a horizontal line using the border color from the current theme.
 */
export class DynamicBorder implements Component {
  private colorFn: (str: string) => string;

  constructor(colorFn?: (str: string) => string) {
    this.colorFn = colorFn ?? ((str) => getTheme().fg("border", str));
  }

  invalidate(): void {}

  render(width: number): string[] {
    return [this.colorFn("─".repeat(Math.max(1, width)))];
  }
}
