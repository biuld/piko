// ============================================================================
// Scroll controller — auto-scroll, manual scroll, bottom anchor management
// ============================================================================

import type { TuiTimelineState } from "./types.js";

export class ScrollController {
  private state: Pick<
    TuiTimelineState,
    "anchor" | "anchorItemId" | "atBottom" | "userScrolled" | "pendingNewItems"
  > = {
    anchor: "bottom",
    atBottom: true,
    userScrolled: false,
    pendingNewItems: 0,
  };

  /**
   * Call when the user manually scrolls away from bottom.
   */
  onUserScrolled(nowAtBottom: boolean): void {
    if (!nowAtBottom) {
      if (this.state.atBottom) {
        this.state.userScrolled = true;
        this.state.anchor = "manual";
      }
      this.state.atBottom = false;
    } else {
      this.state.atBottom = true;
      this.state.pendingNewItems = 0;
    }
  }

  /**
   * Call when new content arrives (message delta, tool call, etc).
   * Returns whether auto-scroll should happen.
   */
  onNewContent(): { shouldScroll: boolean; pendingCount: number } {
    if (this.state.anchor === "bottom" && this.state.atBottom) {
      return { shouldScroll: true, pendingCount: 0 };
    }

    this.state.pendingNewItems++;
    return { shouldScroll: false, pendingCount: this.state.pendingNewItems };
  }

  /**
   * Jump to latest (bottom).
   */
  jumpToLatest(): void {
    this.state.anchor = "bottom";
    this.state.atBottom = true;
    this.state.userScrolled = false;
    this.state.pendingNewItems = 0;
  }

  /**
   * Anchor to a specific item.
   */
  anchorToItem(itemId: string): void {
    this.state.anchor = "item";
    this.state.anchorItemId = itemId;
  }

  /**
   * Restore to bottom anchor from item anchor.
   */
  restoreFromItemAnchor(): void {
    if (this.state.anchor === "item") {
      this.state.anchor = this.state.userScrolled ? "manual" : "bottom";
      this.state.anchorItemId = undefined;
    }
  }

  /**
   * Get current scroll state.
   */
  getState() {
    return { ...this.state };
  }
}
