// ============================================================================
// piko-host-tui — Public API
// ============================================================================

// Agent activity presentation
export type {
  AgentPanelMode,
  AgentPanelSelectEvent,
  AgentPanelViewModel,
  AgentPlanStepViewModel,
  AgentTaskViewModel,
} from "./agents/index.js";
export { buildAgentPanelRows, selectPlanSummary } from "./agents/index.js";
export type { TuiModelCatalog, TuiResolvedModel } from "./app/model-catalog.js";
export type { TuiPreferencesData } from "./app/tui-preferences.js";
export { TuiPreferences } from "./app/tui-preferences.js";
export type { RunTuiOptions } from "./app/types.js";
// Commands subsystem
export type {
  AutocompleteItem,
  CommandAvailability,
  CommandContext,
  CommandDefinition,
} from "./commands/index.js";
export { CommandRegistry, createBuiltinCommands, SlashCommandProvider } from "./commands/index.js";
// Focus subsystem
export type {
  FocusNode,
  FocusOwner,
  FocusRegion,
  FocusResult,
  KeyEvent,
  TuiFocusState,
} from "./focus/index.js";
export { FocusManager } from "./focus/index.js";
// Keymap subsystem
export type {
  AppKeybindingId,
  KeybindingEntry,
  KeybindingId,
  KeyCombo,
  TuiKeybindingId,
} from "./keymap/index.js";
export { formatKeyCombo, KeymapManager, keyComboMatches } from "./keymap/index.js";
// Notifications subsystem
export type {
  NotificationEvent,
  NotificationFilter,
  NotificationSeverity,
  NotificationSource,
  NotifyInput,
  TuiNotification,
  TuiNotificationState,
} from "./notifications/index.js";
export { NotificationCenter } from "./notifications/index.js";
// OpenTUI runtime
export { launchOpenTui } from "./opentui-runtime.js";
// Renderer
export type { AppProps, TuiStore } from "./renderer/opentui/index.js";
export { AgentPanel, App, createDefaultStore, createTuiStore } from "./renderer/opentui/index.js";
export type {
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
  selectStatus,
  selectVisibleMessages,
  tuiReducer,
} from "./state/index.js";
// Surfaces subsystem
export type {
  SurfaceContext,
  SurfaceSlot,
  SurfaceState,
} from "./surfaces/index.js";
export { SurfaceManager } from "./surfaces/index.js";
// Theme
export type { ResolvedTuiTheme } from "./theme/index.js";
export { getDefaultTheme } from "./theme/index.js";
// Timeline subsystem
export type {
  TimelineAnchor,
  TimelineItem,
  TimelineItemKind,
  TimelineLayout,
  TuiTimelineState,
} from "./timeline/index.js";
export {
  createDefaultTimelineState,
  ScrollController,
  timelineReducer,
} from "./timeline/index.js";
