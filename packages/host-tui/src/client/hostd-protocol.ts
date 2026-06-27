// ============================================================================
// hostd-protocol — TUI ↔ hostd wire protocol types
//
// Mirrors packages/host-protocol/src/lib.rs (Rust).
// ============================================================================

// ============================================================================
// Basic ID types
// ============================================================================

export type CommandId = string;
export type SessionId = string;
export type TurnId = string;
export type MessageId = string;
export type ToolCallId = string;
export type ApprovalId = string;
export type TaskId = string;
export type AgentId = string;

// ============================================================================
// HostCommand — TUI → hostd requests (snake_case, matches Rust HostCommand)
// ============================================================================

export type HostCommand =
  | { type: "session_create"; command_id: CommandId; cwd: string }
  | { type: "session_open"; command_id: CommandId; session_id: SessionId }
  | { type: "session_list"; command_id: CommandId }
  | { type: "session_fork"; command_id: CommandId; session_id: SessionId; entry_id?: string }
  | { type: "session_import"; command_id: CommandId; path: string }
  | { type: "session_rename"; command_id: CommandId; session_id: SessionId; name: string }
  | { type: "session_delete"; command_id: CommandId; session_id: SessionId }
  | { type: "session_navigate"; command_id: CommandId; session_id: SessionId; entry_id: string }
  | { type: "turn_submit"; command_id: CommandId; session_id: SessionId; text: string }
  | { type: "turn_cancel"; command_id: CommandId; session_id: SessionId; turn_id: TurnId }
  | {
      type: "approval_respond";
      command_id: CommandId;
      session_id: SessionId;
      approval_id: ApprovalId;
      decision: ApprovalDecision;
      note?: string;
    }
  | { type: "state_snapshot"; command_id: CommandId; session_id: SessionId }
  | { type: "events_resume"; command_id: CommandId; session_id: SessionId; after_seq: number }
  | {
      type: "config_set";
      command_id: CommandId;
      default_provider?: string;
      default_model?: string;
      default_thinking_level?: string;
    };

// ============================================================================
// CommandAck — hostd → TUI (not domain events, RPC-level)
// ============================================================================

export type CommandAck =
  | { type: "command_accepted"; command_id: CommandId }
  | { type: "command_rejected"; command_id: CommandId; reason: string };

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

export interface Usage {
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
  total_tokens: number;
  cost: {
    input: number;
    output: number;
    cache_read: number;
    cache_write: number;
    total: number;
  };
}

// ============================================================================
// HostEvent — 21 variants (snake_case, matches Rust HostEvent)
// ============================================================================

export type HostEvent =
  // ═══ Domain: Messages (3) ═══
  | {
      type: "user_message_submitted";
      session_id: SessionId;
      message_id: MessageId;
      task_id: TaskId;
      text: string;
      timestamp: number;
    }
  | {
      type: "assistant_message_completed";
      session_id: SessionId;
      message_id: MessageId;
      task_id: TaskId;
      agent_id: AgentId;
      text: string;
      tool_calls: ToolCallRef[];
      model: string;
      provider: string;
      usage?: Usage;
      timestamp: number;
    }
  | {
      type: "tool_result_committed";
      session_id: SessionId;
      message_id: MessageId;
      task_id: TaskId;
      agent_id: AgentId;
      tool_call_id: ToolCallId;
      tool_name: string;
      content: unknown;
      is_error: boolean;
      timestamp: number;
    }
  // ═══ Domain: Turn (4) ═══
  | {
      type: "turn_started";
      session_id: SessionId;
      turn_id: TurnId;
      root_task_id: TaskId;
      timestamp: number;
    }
  | {
      type: "turn_completed";
      session_id: SessionId;
      turn_id: TurnId;
      total_tasks: number;
      timestamp: number;
    }
  | {
      type: "turn_failed";
      session_id: SessionId;
      turn_id: TurnId;
      error: string;
      timestamp: number;
    }
  | { type: "turn_cancelled"; session_id: SessionId; turn_id: TurnId; timestamp: number }
  // ═══ Domain: Task (8) ═══
  | {
      type: "task_created";
      session_id: SessionId;
      task_id: TaskId;
      agent_id: AgentId;
      parent_task_id: TaskId | null;
      source_agent_id: AgentId | null;
      prompt: string;
      turn_id: TurnId;
      timestamp: number;
    }
  | {
      type: "task_started";
      session_id: SessionId;
      task_id: TaskId;
      agent_id: AgentId;
      timestamp: number;
    }
  | {
      type: "task_completed";
      session_id: SessionId;
      task_id: TaskId;
      agent_id: AgentId;
      total_steps: number;
      summary: string;
      final_status: string;
      timestamp: number;
    }
  | {
      type: "task_failed";
      session_id: SessionId;
      task_id: TaskId;
      agent_id: AgentId;
      error: string;
      timestamp: number;
    }
  | {
      type: "task_cancelled";
      session_id: SessionId;
      task_id: TaskId;
      agent_id: AgentId;
      timestamp: number;
    }
  | {
      type: "task_transcript_committed";
      session_id: SessionId;
      task_id: TaskId;
      agent_id: AgentId;
      parent_task_id: TaskId;
      messages: unknown[];
      summary: string;
      final_status: string;
      timestamp: number;
    }
  | {
      type: "task_joined";
      session_id: SessionId;
      task_id: TaskId;
      parent_task_id: TaskId;
      result: unknown;
      timestamp: number;
    }
  | {
      type: "task_steered";
      session_id: SessionId;
      task_id: TaskId;
      source_task_id: TaskId;
      source_agent_id: AgentId;
      message: string;
      timestamp: number;
    }
  // ═══ Domain: Session & Config (3) ═══
  | { type: "session_created"; session_id: SessionId; cwd: string; timestamp: number }
  | {
      type: "queue_update";
      session_id: SessionId;
      steer_count: number;
      follow_up_count: number;
      next_turn_count: number;
      steer_preview?: string;
      follow_up_preview?: string;
    }
  | {
      type: "model_config_changed";
      session_id: SessionId;
      model_id: string;
      provider: string;
      timestamp: number;
    }
  | {
      type: "session_opened";
      session_id: SessionId;
      snapshot: HostSessionSnapshot;
      timestamp: number;
    }
  | { type: "session_listed"; sessions: SessionSummary[]; timestamp: number }
  | {
      type: "state_snapshot";
      session_id: SessionId;
      snapshot: HostSessionSnapshot;
      timestamp: number;
    }
  // ═══ Streaming: Message (4) ═══
  | {
      type: "message_start";
      task_id: TaskId;
      agent_id: AgentId;
      message_id: MessageId;
      role: MessageRole;
    }
  | {
      type: "message_end";
      task_id: TaskId;
      agent_id: AgentId;
      message_id: MessageId;
      stop_reason?: string;
    }
  | { type: "text_delta"; task_id: TaskId; agent_id: AgentId; message_id: MessageId; delta: string }
  | {
      type: "thinking_delta";
      task_id: TaskId;
      agent_id: AgentId;
      message_id: MessageId;
      delta: string;
    }
  // ═══ Streaming: Tool (2) ═══
  | {
      type: "tool_start";
      task_id: TaskId;
      agent_id: AgentId;
      tool_call_id: ToolCallId;
      tool_name: string;
      args: unknown;
      parent_message_id?: MessageId;
    }
  | {
      type: "tool_end";
      task_id: TaskId;
      agent_id: AgentId;
      tool_call_id: ToolCallId;
      tool_name: string;
      result: unknown;
      is_error: boolean;
    }
  // ═══ Streaming: Approval (2) ═══
  | {
      type: "approval_requested";
      task_id: TaskId;
      agent_id: AgentId;
      approval_id: ApprovalId;
      tool_name: string;
      tool_args: unknown;
    }
  | {
      type: "approval_resolved";
      task_id: TaskId;
      agent_id: AgentId;
      approval_id: ApprovalId;
      decision: ApprovalDecision;
    };

// ============================================================================
// Helpers
// ============================================================================

const DOMAIN_EVENT_TYPES = new Set([
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
  "session_opened",
  "session_listed",
  "state_snapshot",
  "queue_update",
  "model_config_changed",
]);

export interface HostMessage {
  id: string;
  role: MessageRole;
  text: string;
}

export interface SessionSummary {
  session_id: SessionId;
  cwd: string;
  seq: number;
  name?: string;
}

export interface HostSessionSnapshot {
  session_id: SessionId;
  cwd: string;
  seq: number;
  messages: HostMessage[];
  active_turn: TurnSnapshot | null;
  pending_approvals: ApprovalSnapshot[];
  name?: string;
}

export interface TurnSnapshot {
  turn_id: TurnId;
  status:
    | "idle"
    | "running"
    | "waiting_for_approval"
    | "cancelling"
    | "completed"
    | "failed"
    | "cancelled";
  assistant_text: string;
  tool_calls: ToolCallSnapshot[];
}

export interface ToolCallSnapshot {
  tool_call_id: ToolCallId;
  name: string;
  status: "running" | "completed" | "failed";
  result?: unknown;
}

export interface ApprovalSnapshot {
  approval_id: ApprovalId;
  request: unknown;
  status: "pending" | "approved" | "rejected";
}

export function isDomainEvent(event: HostEvent): boolean {
  return DOMAIN_EVENT_TYPES.has(event.type);
}

export function isStreamingEvent(event: HostEvent): boolean {
  return !isDomainEvent(event);
}
