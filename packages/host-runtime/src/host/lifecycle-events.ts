/**
 * Host-level lifecycle events emitted by runScheduler.
 *
 * These live above the stateless Engine boundary — the Engine only produces
 * EngineEvent (deltas, tool start/end, errors), while the Host wraps those
 * inside turn/message/agent lifecycle events that carry session-level
 * semantics (queue updates, save points, settled state).
 *
 * Rich lifecycle ensures that user messages, assistant messages, tool results,
 * and failure messages all have observable host-level start/update/end boundaries.
 * This allows TUI, session storage, and future RPC/print/extension consumers to
 * share a single contract rather than depending on raw EngineEvent details.
 */

/**
 * Lightweight message descriptor used in lifecycle events.
 * Contains enough information to identify and display a message without
 * requiring the full pi-ai Message shape (which includes provider-specific
 * metadata like api, usage, stopReason that may not be available for
 * synthetic messages like failures).
 */
export interface LifecycleMessage {
  role: "user" | "assistant" | "toolResult";
  content: string;
  /** Present for engine-emitted messages, absent for synthetic ones. */
  messageId?: string;
}

/**
 * Events produced by runScheduler during agent execution.
 *
 * Full lifecycle order for a normal single-turn run:
 *   agent_start → turn_start → message_start → (message_update*) → message_end
 *            → (tool_execution_start → tool_execution_end)*
 *            → turn_end → save_point → settled → agent_end
 *
 * Multi-turn (follow-up / next-turn queues extend this):
 *   agent_start → turn_start → ... → turn_end → save_point
 *            → (follow-up) → turn_start → ... → turn_end → save_point
 *            → settled → agent_end
 *
 * Abort / error:
 *   agent_start → turn_start → (failure message_start/message_end) → failure → settled
 */
export type HostLifecycleEvent =
  | AgentStartEvent
  | TurnStartEvent
  | TurnEndEvent
  | QueueUpdateEvent
  | SavePointEvent
  | SettledEvent
  | AgentEndEvent
  | FailureEvent
  // Rich message lifecycle
  | MessageStartEvent
  | MessageUpdateEvent
  | MessageEndEvent
  // Rich tool execution lifecycle
  | ToolExecutionStartEvent
  | ToolExecutionUpdateEvent
  | ToolExecutionEndEvent;

/** Emitted once when runScheduler begins. */
export interface AgentStartEvent {
  type: "agent_start";
  runId: string;
}

/** Emitted at the start of each inner-loop turn. */
export interface TurnStartEvent {
  type: "turn_start";
  /** 0-based index of the current turn within this run. */
  turnIndex: number;
}

/** Emitted at the end of each inner-loop turn. */
export interface TurnEndEvent {
  type: "turn_end";
  /** 0-based index of the turn that just completed. */
  turnIndex: number;
}

/** Emitted when any of the steering/follow-up/next-turn queues change. */
export interface QueueUpdateEvent {
  type: "queue_update";
  steerCount: number;
  followUpCount: number;
  nextTurnCount: number;
  /** Optional preview of the first pending steering message (truncated). */
  steerPreview?: string;
  /** Optional preview of the first pending follow-up message (truncated). */
  followUpPreview?: string;
  /** Optional preview of the first pending next-turn message (truncated). */
  nextTurnPreview?: string;
}

/** Emitted after each turn-end, before the next turn begins. */
export interface SavePointEvent {
  type: "save_point";
  /** Whether any pending session writes were flushed. */
  hadPendingWrites: boolean;
}

/** Emitted when the agent is fully settled — no more turns pending. */
export interface SettledEvent {
  type: "settled";
  /** Number of next-turn messages still queued for a future run. */
  nextTurnCount: number;
}

/** Emitted once when runScheduler completes (success, max_steps, context_overflow). */
export interface AgentEndEvent {
  type: "agent_end";
  /** Run completion status. */
  status: "completed" | "max_steps" | "context_overflow";
  /** Total steps executed. */
  totalSteps: number;
}

/** Emitted when the run fails irrecoverably (error or abort). */
export interface FailureEvent {
  type: "failure";
  /** Error message, if available. */
  error?: string;
  /** Whether the run was aborted by signal. */
  aborted: boolean;
}

// ---- Rich message lifecycle ----

/**
 * Emitted when the LLM starts generating a new message.
 * This happens before the first `message_delta` engine event.
 */
export interface MessageStartEvent {
  type: "message_start";
  /** Unique ID of the message being generated. */
  messageId: string;
  /** The role of the message (e.g. "assistant", "user"). */
  role: "user" | "assistant" | "toolResult";
}

/**
 * Emitted for each streaming delta from the LLM.
 * Wraps `message_delta` and `thinking_delta` engine events into a host-level event.
 *
 * Use `isThinking` to distinguish between regular text deltas and reasoning/thinking deltas.
 */
export interface MessageUpdateEvent {
  type: "message_update";
  /** The message being updated. */
  messageId: string;
  /** The text delta content. */
  delta: string;
  /** Whether this delta is reasoning/thinking content (true) or regular text (false). */
  isThinking: boolean;
}

/**
 * Emitted when the LLM finishes generating a message.
 *
 * Uses LifecycleMessage (a lightweight descriptor) rather than the full pi-ai Message
 * type, so synthetic messages (e.g., failure notifications) can be represented
 * without requiring provider-specific metadata like api, usage, stopReason.
 */
export interface MessageEndEvent {
  type: "message_end";
  /** The completed message as a lightweight descriptor. */
  message: LifecycleMessage;
}

// ---- Rich tool execution lifecycle ----

/**
 * Emitted when the engine begins executing a tool.
 * Wraps the engine's `tool_call_start` event.
 */
export interface ToolExecutionStartEvent {
  type: "tool_execution_start";
  /** Unique ID of this tool call. */
  toolCallId: string;
  /** Name of the tool being executed. */
  toolName: string;
  /** Arguments passed to the tool. */
  args: Record<string, unknown>;
}

/**
 * Emitted during tool execution with partial/intermediate results.
 * Engines may emit this for long-running tools that produce streaming output.
 */
export interface ToolExecutionUpdateEvent {
  type: "tool_execution_update";
  /** Unique ID of this tool call. */
  toolCallId: string;
  /** Name of the tool being executed. */
  toolName: string;
  /** Arguments passed to the tool (carried through for context). */
  args: Record<string, unknown>;
  /** Partial result emitted during tool execution. */
  partialResult: unknown;
}

/**
 * Emitted when the engine completes (or fails) executing a tool.
 * Wraps the engine's `tool_call_end` event.
 */
export interface ToolExecutionEndEvent {
  type: "tool_execution_end";
  /** Unique ID of this tool call. */
  toolCallId: string;
  /** Name of the tool that was executed. */
  toolName: string;
  /** The tool execution result (or error message if isError). */
  result: unknown;
  /** Whether the tool execution resulted in an error. */
  isError: boolean;
}
