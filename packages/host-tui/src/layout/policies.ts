// ============================================================================
// Layout Policies — derive layout state from domain + view state + viewport
// ============================================================================

import type { LayoutMode, TuiState } from "../state/state.js";
import { detectBottomBarDensity, detectLayoutMode } from "./model.js";

// ============================================================================
// Policy: apply layout decisions based on current state
// ============================================================================

/**
 * Apply layout policies to produce updated TuiLayoutState.
 * Called after any event that might affect layout (resize, overlay change, etc.)
 */
export function applyLayoutPolicies(state: TuiState): TuiState {
  const { width, height } = state.layout.viewport;

  const mode = detectLayoutMode(width, height);
  const density = detectBottomBarDensity(width);
  const activeRegion = state.layout.activeRegion;
  const visibleFields = bottomBarFieldsForDensity(density);

  return {
    ...state,
    layout: {
      ...state.layout,
      viewport: { width, height },
      mode,
      activeRegion,
      bottomBar: { density, visibleFields },
    },
  };
}

/**
 * Determine scroll behavior during streaming.
 * If user has manually scrolled away from bottom, don't auto-scroll.
 */
export function shouldAutoScroll(state: TuiState): boolean {
  return state.timeline.anchor === "bottom";
}

/**
 * Get the maximum editor rows based on layout mode.
 */
export function getEditorMaxRows(mode: LayoutMode): number {
  switch (mode) {
    case "regular":
      return 10;
    case "compact":
      return 5;
    case "minimal":
      return 3;
  }
}

/**
 * Get the bottom bar rows based on layout density.
 */
export function getBottomBarRows(mode: LayoutMode): number {
  switch (mode) {
    case "regular":
      return 4;
    case "compact":
      return 2;
    case "minimal":
      return 1;
  }
}

// ============================================================================
// Helpers
// ============================================================================

function bottomBarFieldsForDensity(
  density: string,
): Array<"model" | "session" | "branch" | "tokens" | "cost" | "cwd" | "mode" | "hints"> {
  switch (density) {
    case "full":
      return ["model", "session", "branch", "tokens", "cost", "cwd", "hints"];
    case "compact":
      return ["model", "tokens", "cost", "cwd", "hints"];
    case "minimal":
      return ["model", "cwd"];
    default:
      return ["model", "cwd"];
  }
}
