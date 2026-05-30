import { type Component, truncateToWidth } from "@earendil-works/pi-tui";
import { getTheme } from "../theme.js";

/**
 * Multi-slot status line displayed between spinner and footer.
 * Extensions can set status text by key; empty when no slots are active.
 */
export class StatusLine implements Component {
  private slots: Map<string, string> = new Map();

  /** Set a status slot. Pass undefined to clear. */
  set(key: string, text: string | undefined): void {
    if (text === undefined) {
      this.slots.delete(key);
    } else {
      this.slots.set(key, text);
    }
  }

  /** Clear all slots */
  clear(): void {
    this.slots.clear();
  }

  /** Get sorted entries for footer display */
  getEntries(): string[] {
    return [...this.slots.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([, text]) => text);
  }

  invalidate(): void {}

  render(width: number): string[] {
    if (this.slots.size === 0) return [];

    const t = getTheme();
    // Sort by key for stable display order
    const entries = [...this.slots.entries()].sort(([a], [b]) => a.localeCompare(b));
    const joined = entries.map(([, text]) => text).join(" │ ");
    return [truncateToWidth(t.fg("muted", joined), width)];
  }
}
