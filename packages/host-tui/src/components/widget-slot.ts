import type { Component, TUI } from "@earendil-works/pi-tui";
import type { WidgetContent } from "../extensions/index.js";
import { getTheme } from "../theme.js";

/**
 * Renders extension widgets above or below the editor.
 * Supports both static string arrays and component factories.
 */
export class WidgetSlot implements Component {
  private widgets: Map<
    string,
    { content: WidgetContent; factoryResult?: Component & { dispose?(): void } }
  > = new Map();
  private tui: TUI | null = null;

  bind(tui: TUI): void {
    this.tui = tui;
  }

  set(key: string, content: WidgetContent | undefined): void {
    if (content === undefined) {
      const existing = this.widgets.get(key);
      existing?.factoryResult?.dispose?.();
      this.widgets.delete(key);
    } else {
      this.widgets.set(key, { content });
    }
  }

  invalidate(): void {
    for (const w of this.widgets.values()) {
      w.factoryResult?.invalidate?.();
    }
  }

  render(width: number): string[] {
    if (this.widgets.size === 0) return [];

    const t = getTheme();
    const lines: string[] = [];
    const entries = [...this.widgets.entries()].sort(([a], [b]) => a.localeCompare(b));

    for (const [, w] of entries) {
      const content = w.content;
      if (typeof content === "function" && this.tui) {
        if (!w.factoryResult) {
          w.factoryResult = content(this.tui, t);
        }
        lines.push(...w.factoryResult.render(width));
      } else if (Array.isArray(content)) {
        for (const line of content) {
          lines.push(t.fg("dim", line));
        }
      }
    }

    if (lines.length > 0) {
      lines.push("");
    }

    return lines;
  }
}
