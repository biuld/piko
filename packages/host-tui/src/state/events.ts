// ============================================================================
// TUI Events — all external changes flow through events
// Stream handlers dispatch events; reducers produce new state.
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig, Message } from "piko-engine-protocol";
import type { TuiNotification } from "../notifications/types.js";
import type { TuiSurfaceState } from "../surfaces/types.js";
import type { TuiMessageViewModel } from "./state.js";

// ============================================================================
// Event type definitions
// ============================================================================

export interface UserInputChangedEvent {
  type: "user_input_changed";
  text: string;
}

export interface UserSubmittedEvent {
  type: "user_submitted";
  text: string;
}

export interface StreamStartedEvent {
  type: "stream_started";
}

export interface AssistantDeltaEvent {
  type: "assistant_delta";
  delta: string;
}

export interface ThinkingDeltaEvent {
  type: "thinking_delta";
  delta: string;
}

export interface ToolCallStartedEvent {
  type: "tool_call_started";
  id: string;
  name: string;
  args: unknown;
}

export interface ToolCallEndedEvent {
  type: "tool_call_ended";
  id: string;
  name: string;
  result: unknown;
  isError: boolean;
}

export interface TurnFinishedEvent {
  type: "turn_finished";
  status: string;
  transcript: Message[];
}

export interface TurnFailedEvent {
  type: "turn_failed";
  error: string;
}

export interface QueueUpdateEvent {
  type: "queue_update";
  steerCount: number;
  steerPreview?: string;
  followUpCount: number;
  followUpPreview?: string;
}

export interface LayoutResizedEvent {
  type: "layout_resized";
  width: number;
  height: number;
}

export interface RegionFocusedEvent {
  type: "region_focused";
  region: "chat" | "editor";
}

export interface ChatScrolledEvent {
  type: "chat_scrolled";
  anchor: "bottom" | "selection" | "manual";
}

export interface ToolBlockToggledEvent {
  type: "tool_block_toggled";
  toolCallId: string;
}

export interface ModelChangedEvent {
  type: "model_changed";
  model: Model<string>;
  providerConfig: EngineProviderConfig;
}

export interface SessionResumedEvent {
  type: "session_resumed";
  sessionId: string;
  sessionName?: string;
  transcript: TuiMessageViewModel[];
}

export interface SessionForkedEvent {
  type: "session_forked";
  sessionId: string;
}

export interface SessionInfoUpdatedEvent {
  type: "session_info_updated";
  sessionId?: string;
  sessionName?: string;
  messageCount?: number;
}

export interface UsageUpdatedEvent {
  type: "usage_updated";
  inputTokens?: number;
  outputTokens?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  totalCost?: number;
  contextWindow?: number;
  contextPercent?: number;
}

export interface ThinkingLevelChangedEvent {
  type: "thinking_level_changed";
  level: string;
}

export interface ExtensionStatusSetEvent {
  type: "extension_status_set";
  key: string;
  text: string | undefined;
}

export interface AbortedEvent {
  type: "aborted";
}

// ---- New subsystem events ----

export interface NotificationAddedEvent {
  type: "notification_added";
  notification: TuiNotification;
}

export interface NotificationClearedEvent {
  type: "notification_cleared";
  id?: string;
}

export interface NotificationReadEvent {
  type: "notification_read";
  id?: string;
}

export interface SurfaceOpenedEvent {
  type: "surface_opened";
  surface: TuiSurfaceState;
}

export interface SurfaceClosedEvent {
  type: "surface_closed";
  surfaceId: string;
}

export interface TimelineScrolledEvent {
  type: "timeline_scrolled";
  anchor: "bottom" | "manual" | "item";
  atBottom: boolean;
}

export interface TimelineItemToggledEvent {
  type: "timeline_item_toggled";
  itemId: string;
}

export interface TimelineToolToggledEvent {
  type: "timeline_tool_toggled";
  toolCallId: string;
}

export interface TimelinePendingUpdateEvent {
  type: "timeline_pending_update";
  pendingNewItems: number;
}

export interface FocusChangedEvent {
  type: "focus_changed";
  activeOwnerId: string;
  region: "editor" | "autocomplete" | "chat" | "surface" | "confirm";
}

export interface AutocompleteActiveEvent {
  type: "autocomplete_active";
  active: boolean;
  selectedIndex?: number;
}

export interface AutocompleteNavigateEvent {
  type: "autocomplete_navigate";
  delta: number;
}

export interface AutocompleteAcceptEvent {
  type: "autocomplete_accept";
}

// ============================================================================
// Union type
// ============================================================================

export type TuiEvent =
  | UserInputChangedEvent
  | UserSubmittedEvent
  | StreamStartedEvent
  | AssistantDeltaEvent
  | ThinkingDeltaEvent
  | ToolCallStartedEvent
  | ToolCallEndedEvent
  | TurnFinishedEvent
  | TurnFailedEvent
  | QueueUpdateEvent
  | LayoutResizedEvent
  | RegionFocusedEvent
  | ChatScrolledEvent
  | ToolBlockToggledEvent
  | ModelChangedEvent
  | SessionResumedEvent
  | SessionForkedEvent
  | SessionInfoUpdatedEvent
  | UsageUpdatedEvent
  | ThinkingLevelChangedEvent
  | ExtensionStatusSetEvent
  | AbortedEvent
  // New subsystem events
  | NotificationAddedEvent
  | NotificationClearedEvent
  | NotificationReadEvent
  | SurfaceOpenedEvent
  | SurfaceClosedEvent
  | TimelineScrolledEvent
  | TimelineItemToggledEvent
  | TimelineToolToggledEvent
  | TimelinePendingUpdateEvent
  | FocusChangedEvent
  | AutocompleteActiveEvent
  | AutocompleteNavigateEvent
  | AutocompleteAcceptEvent;
