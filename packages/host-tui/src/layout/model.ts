// ============================================================================
// Layout Model — terminal layout structure and measurements
// ============================================================================

import type { BottomBarDensity, LayoutMode, TuiLayoutState } from "../state/state.js";

// ============================================================================
// Region sizing
// ============================================================================

/**
 * Calculates the number of rows allocated to each region.
 * Returns heights for: chat, status, editor, bottomBar, plus remaining for chat.
 */
export interface RegionHeights {
  chat: number;
  status: number;
  editor: number;
  bottomBar: number;
  /** Total height consumed by non-chat regions */
  overhead: number;
}

export function computeRegionHeights(
  viewport: { width: number; height: number },
  mode: LayoutMode,
  editorLines: number,
): RegionHeights {
  const totalHeight = viewport.height;

  // Status line: 1 row when active, 0 otherwise
  const status = mode === "minimal" ? 0 : 1;

  // Editor: dynamic based on content but capped
  const maxEditor = mode === "minimal" ? 3 : mode === "compact" ? 5 : 10;
  const editor = Math.min(editorLines, maxEditor);

  // Bottom bar: varies by mode
  const bottomBar = mode === "minimal" ? 1 : mode === "compact" ? 2 : 3;

  const overhead = status + editor + bottomBar;
  const chat = Math.max(1, totalHeight - overhead);

  return { chat, status, editor, bottomBar, overhead };
}

// ============================================================================
// Layout mode detection from viewport
// ============================================================================

export function detectLayoutMode(width: number, height: number): LayoutMode {
  if (width >= 100 && height >= 24) return "regular";
  if (width >= 60 && height >= 16) return "compact";
  return "minimal";
}

export function detectBottomBarDensity(width: number): BottomBarDensity {
  if (width >= 120) return "full";
  if (width >= 80) return "compact";
  return "minimal";
}

// ============================================================================
// Layout state factory
// ============================================================================

export interface LayoutStateParams {
  width: number;
  height: number;
}

export function createLayoutState(params: LayoutStateParams): TuiLayoutState {
  const mode = detectLayoutMode(params.width, params.height);
  const density = detectBottomBarDensity(params.width);

  return {
    viewport: { width: params.width, height: params.height },
    mode,
    activeRegion: "editor",
    bottomBar: {
      density,
      visibleFields: bottomBarFieldsForDensity(density),
    },
  };
}

function bottomBarFieldsForDensity(
  density: BottomBarDensity,
): Array<"model" | "session" | "branch" | "tokens" | "cost" | "cwd" | "mode" | "hints"> {
  switch (density) {
    case "full":
      return ["model", "session", "branch", "tokens", "cost", "cwd", "hints"];
    case "compact":
      return ["model", "tokens", "cost", "cwd", "hints"];
    case "minimal":
      return ["model", "cwd"];
  }
}
