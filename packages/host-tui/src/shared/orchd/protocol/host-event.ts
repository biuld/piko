// ============================================================================
// Unified HostEvent — single source of truth for all Host ↔ TUI communication.
//
// Architecture:
//   Domain events (21): persisted to JSONL session journal, used for state rebuild.
//   Streaming events (6): real-time push only, never persisted.
//
// Every domain event carries `session_id` + `timestamp`.
// Every streaming event carries `task_id` + `agent_id`.
// ============================================================================

import type { Message } from "./messages.js";

// ============================================================================
// Companion types
// ============================================================================

export interface ToolCallRef {
  id: string;
  name: string;
  args: unknown;
}

export type MessageRole = "assistant" | "tool_result" | "user";

export type ApprovalDecision = "accept" | "decline" | "accept_session" | "accept_workspace";

// ============================================================================
// HostEvent — discriminated union (21 variants)
// ============================================================================

export type HostEvent =
  // ═══ Domain: Messages (3) ═══
  | HostEventUserMessageSubmitted
  | HostEventAssistantMessageCompleted
  | HostEventToolResultCommitted
  // ═══ Domain: Turn (4) ═══
  | HostEventTurnStarted
  | HostEventTurnCompleted
  | HostEventTurnFailed
  | HostEventTurnCancelled
  // ═══ Domain: Task (8) ═══
  | HostEventTaskCreated
  | HostEventTaskStarted
  | HostEventTaskCompleted
  | HostEventTaskFailed
  | HostEventTaskCancelled
  | HostEventTaskTranscriptCommitted
  | HostEventTaskJoined
  | HostEventTaskSteered
  // ═══ Domain: Session & Config (3) ═══
  | HostEventSessionCreated
  | HostEventQueueUpdate
  | HostEventModelConfigChanged
  // ═══ Streaming: Message (4) ═══
  | HostEventMessageStart
  | HostEventMessageEnd
  | HostEventTextDelta
  | HostEventThinkingDelta
  // ═══ Streaming: Tool (2) ═══
  | HostEventToolStart
  | HostEventToolEnd
  // ═══ Streaming: Approval (2) ═══
  | HostEventApprovalRequested
  | HostEventApprovalResolved;

// ============================================================================
// Domain event interfaces
// ============================================================================

export interface HostEventUserMessageSubmitted {
  type: "user_message_submitted";
  session_id: string;
  message_id: string;
  task_id: string;
  text: string;
  timestamp: number;
}

export interface HostEventAssistantMessageCompleted {
  type: "assistant_message_completed";
  session_id: string;
  message_id: string;
  task_id: string;
  agent_id: string;
  text: string;
  tool_calls: ToolCallRef[];
  model: string;
  provider: string;
  usage?: Usage;
  timestamp: number;
}

export interface HostEventToolResultCommitted {
  type: "tool_result_committed";
  session_id: string;
  message_id: string;
  task_id: string;
  agent_id: string;
  tool_call_id: string;
  tool_name: string;
  content: unknown;
  is_error: boolean;
  timestamp: number;
}

// ============================================================================
// Turn lifecycle event interfaces
// ============================================================================

export interface HostEventTurnStarted {
  type: "turn_started";
  session_id: string;
  turn_id: string;
  root_task_id: string;
  timestamp: number;
}

export interface HostEventTurnCompleted {
  type: "turn_completed";
  session_id: string;
  turn_id: string;
  total_tasks: number;
  timestamp: number;
}

export interface HostEventTurnFailed {
  type: "turn_failed";
  session_id: string;
  turn_id: string;
  error: string;
  timestamp: number;
}

export interface HostEventTurnCancelled {
  type: "turn_cancelled";
  session_id: string;
  turn_id: string;
  timestamp: number;
}

// ============================================================================
// Task lifecycle event interfaces
// ============================================================================

export interface HostEventTaskCreated {
  type: "task_created";
  session_id: string;
  task_id: string;
  agent_id: string;
  parent_task_id: string | null;
  source_agent_id: string | null;
  prompt: string;
  turn_id: string;
  timestamp: number;
}

export interface HostEventTaskStarted {
  type: "task_started";
  session_id: string;
  task_id: string;
  agent_id: string;
  timestamp: number;
}

export interface HostEventTaskCompleted {
  type: "task_completed";
  session_id: string;
  task_id: string;
  agent_id: string;
  total_steps: number;
  summary: string;
  final_status: string;
  timestamp: number;
}

export interface HostEventTaskFailed {
  type: "task_failed";
  session_id: string;
  task_id: string;
  agent_id: string;
  error: string;
  timestamp: number;
}

export interface HostEventTaskCancelled {
  type: "task_cancelled";
  session_id: string;
  task_id: string;
  agent_id: string;
  timestamp: number;
}

export interface HostEventTaskTranscriptCommitted {
  type: "task_transcript_committed";
  session_id: string;
  task_id: string;
  agent_id: string;
  parent_task_id: string;
  messages: Message[];
  summary: string;
  final_status: string;
  timestamp: number;
}

export interface HostEventTaskJoined {
  type: "task_joined";
  session_id: string;
  task_id: string;
  parent_task_id: string;
  result: unknown;
  timestamp: number;
}

export interface HostEventTaskSteered {
  type: "task_steered";
  session_id: string;
  task_id: string;
  source_task_id: string;
  source_agent_id: string;
  message: string;
  timestamp: number;
}

// ============================================================================
// Session & Config event interfaces
// ============================================================================

export interface HostEventSessionCreated {
  type: "session_created";
  session_id: string;
  cwd: string;
  timestamp: number;
}

export interface HostEventQueueUpdate {
  type: "queue_update";
  session_id: string;
  steer_count: number;
  follow_up_count: number;
  next_turn_count: number;
  steer_preview?: string;
  follow_up_preview?: string;
}

export interface HostEventModelConfigChanged {
  type: "model_config_changed";
  session_id: string;
  model_id: string;
  provider: string;
  timestamp: number;
}

// ============================================================================
// Streaming event interfaces
// ============================================================================

export interface HostEventMessageStart {
  type: "message_start";
  task_id: string;
  agent_id: string;
  message_id: string;
  role: MessageRole;
}

export interface HostEventMessageEnd {
  type: "message_end";
  task_id: string;
  agent_id: string;
  message_id: string;
  stop_reason?: string;
}

export interface HostEventTextDelta {
  type: "text_delta";
  task_id: string;
  agent_id: string;
  message_id: string;
  delta: string;
}

export interface HostEventThinkingDelta {
  type: "thinking_delta";
  task_id: string;
  agent_id: string;
  message_id: string;
  delta: string;
}

export interface HostEventToolStart {
  type: "tool_start";
  task_id: string;
  agent_id: string;
  tool_call_id: string;
  tool_name: string;
  args: unknown;
  parent_message_id?: string;
}

export interface HostEventToolEnd {
  type: "tool_end";
  task_id: string;
  agent_id: string;
  tool_call_id: string;
  tool_name: string;
  result: unknown;
  is_error: boolean;
}

export interface HostEventApprovalRequested {
  type: "approval_requested";
  task_id: string;
  agent_id: string;
  approval_id: string;
  tool_name: string;
  tool_args: unknown;
}

export interface HostEventApprovalResolved {
  type: "approval_resolved";
  task_id: string;
  agent_id: string;
  approval_id: string;
  decision: ApprovalDecision;
}

// ============================================================================
// Usage type (same shape as existing Message.usage)
// ============================================================================

export interface Usage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  totalTokens: number;
  cost: {
    input: number;
    output: number;
    cacheRead: number;
    cacheWrite: number;
    total: number;
  };
}

// ============================================================================
// Helper: event classification
// ============================================================================

const DOMAIN_EVENT_TYPES = new Set<string>([
  "user_message_submitted",
  "assistant_message_completed",
  "tool_result_committed",
  "turn_started",
  "turn_completed",
  "turn_failed",
  "turn_cancelled",
  "task_created",
  "task_started",
  "task_completed",
  "task_failed",
  "task_cancelled",
  "task_transcript_committed",
  "task_joined",
  "task_steered",
  "session_created",
  "queue_update",
  "model_config_changed",
]);

const STREAMING_EVENT_TYPES = new Set<string>([
  "message_start",
  "message_end",
  "text_delta",
  "thinking_delta",
  "tool_start",
  "tool_end",
  "approval_requested",
  "approval_resolved",
]);

/** Returns true if this event type is a domain event (persisted to journal). */
export function isDomainEvent(event: HostEvent): boolean {
  return DOMAIN_EVENT_TYPES.has(event.type);
}

/** Returns true if this event type is a streaming event (real-time only). */
export function isStreamingEvent(event: HostEvent): boolean {
  return STREAMING_EVENT_TYPES.has(event.type);
}

/** All domain event type strings. */
export function domainEventTypes(): ReadonlySet<string> {
  return DOMAIN_EVENT_TYPES;
}

/** All streaming event type strings. */
export function streamingEventTypes(): ReadonlySet<string> {
  return STREAMING_EVENT_TYPES;
}

// ============================================================================
// HostEventListener — subscriber callback
// ============================================================================

export type HostEventListener = (event: HostEvent) => void;
