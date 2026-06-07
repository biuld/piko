// ============================================================================
// Layout module — public API
// ============================================================================

export {
  getTerminalSize,
  measureTextLines,
  truncateToWidth,
  visibleWidth,
} from "./measure.js";
export type { LayoutStateParams, RegionHeights } from "./model.js";
export {
  computeRegionHeights,
  createLayoutState,
  detectBottomBarDensity,
  detectLayoutMode,
} from "./model.js";

export {
  applyLayoutPolicies,
  getBottomBarRows,
  getEditorMaxRows,
  shouldAutoScroll,
} from "./policies.js";
