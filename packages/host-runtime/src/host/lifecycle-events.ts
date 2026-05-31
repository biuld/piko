/**
 * Host-level lifecycle events emitted by runScheduler.
 *
 * These live above the stateless Engine boundary — the Engine only produces
 * EngineEvent (deltas, tool start/end, errors), while the Host wraps those
 * inside turn/message/agent lifecycle events that carry session-level
 * semantics (queue updates, save points, settled state).
 */

/**
 * Events produced by runScheduler during agent execution.
 *
 * Lifecycle order for a normal single-turn run:
 *   agent_start → turn_start → (engine events) → turn_end → settled → agent_end
 *
 * Multi-turn (follow-up / next-turn queues extend this):
 *   agent_start → turn_start → ... → turn_end → (follow-up) → turn_start → ... → turn_end → settled → agent_end
 *
 * Abort / error:
 *   agent_start → turn_start → failure → agent_end
 */
export type HostLifecycleEvent =
  | AgentStartEvent
  | TurnStartEvent
  | TurnEndEvent
  | QueueUpdateEvent
  | SavePointEvent
  | SettledEvent
  | AgentEndEvent
  | FailureEvent;

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
