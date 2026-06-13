// ============================================================================
// TUI Events — all external changes flow through events
// Stream handlers dispatch events; reducers produce new state.
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type { Message, ModelProviderConfig } from "piko-orchestrator-protocol";
import type { TuiNotification } from "../notifications/types.js";
import type { SurfaceState } from "../surfaces/types.js";
import type { TuiMessageViewModel } from "./state.js";

// ============================================================================
// Event type definitions
// ============================================================================

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

export interface ChatScrolledEvent {
  type: "chat_scrolled";
  anchor: "bottom" | "manual";
}

export interface ModelChangedEvent {
  type: "model_changed";
  model: Model<string>;
  providerConfig: ModelProviderConfig;
}

export interface SessionResumedEvent {
  type: "session_resumed";
  sessionId: string;
  sessionName?: string;
  transcript: TuiMessageViewModel[];
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
  surface: SurfaceState;
}

export interface SurfaceClosedEvent {
  type: "surface_closed";
  surfaceId: string;
}

export interface TimelineToggleAllToolsEvent {
  type: "timeline_toggle_all_tools";
}

export interface TimelineJumpLatestEvent {
  type: "timeline_jump_latest";
}

export interface FocusChangedEvent {
  type: "focus_changed";
  activeOwnerId: string;
  region: "editor" | "chat" | "surface" | "confirm";
}

export interface SurfaceUpdatedEvent {
  type: "surface_updated";
}

// ============================================================================
// Union type
// ============================================================================

export type TuiEvent =
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
  | ChatScrolledEvent
  | ModelChangedEvent
  | SessionResumedEvent
  | SessionInfoUpdatedEvent
  | UsageUpdatedEvent
  | ThinkingLevelChangedEvent
  | AbortedEvent
  // New subsystem events
  | NotificationAddedEvent
  | NotificationClearedEvent
  | NotificationReadEvent
  | SurfaceOpenedEvent
  | SurfaceClosedEvent
  | TimelineJumpLatestEvent
  | TimelineToggleAllToolsEvent
  | FocusChangedEvent
  | SurfaceUpdatedEvent;
