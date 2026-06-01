// ============================================================================
// State module — public API
// ============================================================================

export type {
  AbortedEvent,
  AssistantDeltaEvent,
  ChatScrolledEvent,
  ExtensionStatusSetEvent,
  LayoutResizedEvent,
  ModelChangedEvent,
  OverlayClosedEvent,
  OverlayOpenedEvent,
  RegionFocusedEvent,
  SessionForkedEvent,
  SessionInfoUpdatedEvent,
  SessionResumedEvent,
  StreamStartedEvent,
  ThinkingDeltaEvent,
  ThinkingLevelChangedEvent,
  ToolBlockToggledEvent,
  ToolCallEndedEvent,
  ToolCallStartedEvent,
  TuiEvent,
  TurnFailedEvent,
  TurnFinishedEvent,
  UsageUpdatedEvent,
  UserInputChangedEvent,
  UserSubmittedEvent,
} from "./events.js";

export { tuiReducer } from "./reducer.js";
export {
  selectBottomBarDensity,
  selectBottomBarFields,
  selectContextInfo,
  selectFormattedCost,
  selectFormattedInputTokens,
  selectFormattedOutputTokens,
  selectLastMessageIndex,
  selectLayoutMode,
  selectOverlayPlacement,
  selectStatusEntries,
  selectVisibleMessages,
} from "./selectors.js";
export type {
  BottomBarDensity,
  BottomBarField,
  LayoutActiveRegion,
  LayoutMode,
  ToolBlockViewModel,
  TuiExtensionSlots,
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
} from "./state.js";
export { createDefaultTuiState } from "./state.js";
