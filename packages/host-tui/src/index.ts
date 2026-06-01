// ============================================================================
// piko-host-tui — Public API
// ============================================================================

export type { RunTuiOptions } from "./app/types.js";
// OpenTUI runtime
export { launchOpenTui } from "./opentui-runtime.js";

export type {
  ActionContext,
  BottomBarDensity,
  BottomBarField,
  LayoutActiveRegion,
  LayoutMode,
  ToolBlockViewModel,
  TuiEvent,
  TuiInputState,
  TuiLayoutState,
  TuiMessageViewModel,
  TuiModelState,
  TuiOverlayKind,
  TuiOverlayState,
  TuiSessionState,
  TuiState,
  TuiStreamState,
  TuiUsageState,
} from "./state/index.js";
// State model (for consumers that want to embed the state management)
export {
  createDefaultTuiState,
  selectBottomBarDensity,
  selectBottomBarFields,
  selectContextInfo,
  selectFormattedCost,
  selectFormattedInputTokens,
  selectFormattedOutputTokens,
  selectLayoutMode,
  selectOverlayPlacement,
  selectStatusEntries,
  selectVisibleMessages,
  tuiReducer,
} from "./state/index.js";
