// ============================================================================
// Layout module — public API
// ============================================================================

export {
  getTerminalSize,
  measureTextLines,
  truncateToWidth,
  visibleWidth,
} from "./measure.js";

export {
  applyLayoutPolicies,
  getBottomBarRows,
  getEditorMaxRows,
  shouldAutoScroll,
} from "./policies.js";
