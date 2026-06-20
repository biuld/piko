// ============================================================================
// TUI Events — all external changes flow through events
// Stream handlers dispatch events; reducers produce new state.
// ============================================================================

import type { Model } from "@earendil-works/pi-ai";
import type {
  Message,
  ModelProviderConfig,
  RuntimeAssistantMessageEvent,
  RuntimeMessage,
} from "piko-orchestrator-protocol";

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

export interface MessageStartEvent {
  type: "message_start";
  message: RuntimeMessage;
  /** Orchestrator runId for sequence validation. */
  runId?: string;
  /** Event sequence from protocol (eventSeq). */
  eventSeq?: number;
  /** Zero-based turn index. */
  turnIndex?: number;
  /** Stable message position (task-local, informational). */
  messageIndex?: number;
}

export interface MessageUpdateEvent {
  type: "message_update";
  message: RuntimeMessage;
  assistantEvent?: RuntimeAssistantMessageEvent;
  /** Orchestrator runId for sequence validation. */
  runId?: string;
  /** Event sequence from protocol. */
  eventSeq?: number;
  /** Zero-based turn index. */
  turnIndex?: number;
  /** Stable message position (task-local, informational). */
  messageIndex?: number;
}

export interface MessageEndEvent {
  type: "message_end";
  message: RuntimeMessage;
  /** Orchestrator runId for sequence validation. */
  runId?: string;
  /** Event sequence from protocol. */
  eventSeq?: number;
  /** Zero-based turn index. */
  turnIndex?: number;
  /** Stable message position (task-local, informational). */
  messageIndex?: number;
}

export interface ToolCallStartedEvent {
  type: "tool_call_started";
  id: string;
  name: string;
  args: unknown;
  /** Orchestrator runId for sequence validation. */
  runId?: string;
  /** Event sequence from protocol. */
  eventSeq?: number;
  /** Zero-based turn index. */
  turnIndex?: number;
  /** Assistant message containing this tool call. */
  parentMessageId?: string;
  /** Position in parent content blocks. */
  contentIndex?: number;
  /** Dense position among tool calls. */
  toolCallIndex?: number;
}

export interface ToolCallEndedEvent {
  type: "tool_call_ended";
  id: string;
  name: string;
  result: unknown;
  isError: boolean;
  /** Orchestrator runId for sequence validation. */
  runId?: string;
  /** Event sequence from protocol. */
  eventSeq?: number;
  /** Zero-based turn index. */
  turnIndex?: number;
  /** Assistant message containing this tool call. */
  parentMessageId?: string;
  /** Position in parent content blocks. */
  contentIndex?: number;
  /** Dense position among tool calls. */
  toolCallIndex?: number;
}

export interface TurnFinishedEvent {
  type: "turn_finished";
  status: string;
  transcript: Message[];
  /** Event sequence for final commit. */
  eventSeq?: number;
}

export interface TurnFailedEvent {
  type: "turn_failed";
  error: string;
}

export interface QueueUpdateEvent {
  type: "queue_update";
  agentId?: string;
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
  /** If true, transcript has runtime ordering metadata (live session). */
  hasRuntimeOrdering?: boolean;
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

export interface SettingsUpdatedEvent {
  type: "settings_updated";
  settings: Partial<import("./state.js").TuiLayoutState>;
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

export interface EditorDraftReplacement {
  text: string;
  revision: number;
  source: {
    kind: "session_tree";
    sessionId: string;
    entryId: string;
  };
}

export interface TreeNavigationViewResult {
  status: "navigated" | "already_current";
  sessionId: string;
  oldLeafId: string | null;
  newLeafId: string | null;
  selectedEntryId: string;
  transcript: TuiMessageViewModel[];
  editorDraft?: EditorDraftReplacement;
  surfaceId: string;
}

export interface EditorDraftChangedEvent {
  type: "editor_draft_changed";
  text: string;
}

export interface EditorDraftReplacedEvent {
  type: "editor_draft_replaced";
  text: string;
}

export interface TreeNavigationStartedEvent {
  type: "tree_navigation_started";
  operationId: string;
  entryId: string;
}

export interface TreeNavigationSucceededEvent {
  type: "tree_navigation_succeeded";
  operationId: string;
  result: TreeNavigationViewResult;
}

export interface TreeNavigationFailedEvent {
  type: "tree_navigation_failed";
  operationId: string;
  error: string;
}

export interface ViewedAgentChangedEvent {
  type: "viewed_agent_changed";
  agentId: string;
}

export interface AgentExpansionToggledEvent {
  type: "agent_expansion_toggled";
}

export interface ApprovalNeededEvent {
  type: "approval_needed";
  callId: string;
  toolName: string;
  toolArgs: unknown;
}

export interface ApprovalResolvedEvent {
  type: "approval_resolved";
  callId: string;
  decision: "accept" | "decline";
}

export interface StreamSettledEvent {
  type: "stream_settled";
}

// ============================================================================
// Union type
// ============================================================================

export type TuiEvent =
  | StreamSettledEvent
  | UserSubmittedEvent
  | StreamStartedEvent
  | AssistantDeltaEvent
  | ThinkingDeltaEvent
  | MessageStartEvent
  | MessageUpdateEvent
  | MessageEndEvent
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
  | SettingsUpdatedEvent
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
  | SurfaceUpdatedEvent
  | EditorDraftChangedEvent
  | EditorDraftReplacedEvent
  | TreeNavigationStartedEvent
  | TreeNavigationSucceededEvent
  | TreeNavigationFailedEvent
  | ViewedAgentChangedEvent
  | AgentExpansionToggledEvent
  | ApprovalNeededEvent
  | ApprovalResolvedEvent;
