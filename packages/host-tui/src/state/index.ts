// ============================================================================
// State module — public API
// ============================================================================

export type {
  AbortedEvent,
  AssistantDeltaEvent,
  ChatScrolledEvent,
  ExtensionStatusSetEvent,
  FocusChangedEvent,
  LayoutResizedEvent,
  ModelChangedEvent,
  NotificationAddedEvent,
  NotificationClearedEvent,
  NotificationReadEvent,
  RegionFocusedEvent,
  SessionForkedEvent,
  SessionInfoUpdatedEvent,
  SessionResumedEvent,
  StreamStartedEvent,
  SurfaceClosedEvent,
  SurfaceOpenedEvent,
  ThinkingDeltaEvent,
  ThinkingLevelChangedEvent,
  TimelineItemToggledEvent,
  TimelinePendingUpdateEvent,
  TimelineScrolledEvent,
  TimelineToolToggledEvent,
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
  TuiSessionState,
  TuiState,
  TuiStreamState,
  TuiUsageState,
} from "./state.js";
export { createDefaultTuiState } from "./state.js";
