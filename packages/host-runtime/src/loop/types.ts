import type {
  EngineEvent,
  EngineProviderConfig,
  EngineRunSettings,
  EngineStepStatus,
  EngineTool,
  ImageContent,
  StatelessEngine,
} from "piko-engine-protocol";
import type { ApprovalHandler } from "../approval-controller.js";
import type { HostLifecycleEvent } from "../host/lifecycle-events.js";
import type { HostConfig } from "../models/index.js";
import type { SessionState } from "../session/index.js";
import type { PrepareTurnFn } from "../turn-state.js";

// ============================================================================
// Queue message types
// ============================================================================

/** Queue consumption mode. */
export type QueueMode = "one-at-a-time" | "all";

/** A queued message to inject mid-stream (steering). */
export interface SteeringMessage {
  text: string;
  /** Optional images to attach to the steered message. */
  images?: ImageContent[];
}

/** A queued message to inject after the current turn completes. */
export interface FollowUpMessage {
  text: string;
  /** Optional images. */
  images?: ImageContent[];
}

/** A queued message for the next full turn. */
export interface NextTurnMessage {
  text: string;
  /** Optional images. */
  images?: ImageContent[];
}

// ============================================================================
// Scheduler options
// ============================================================================

export interface SchedulerOptions {
  engine: StatelessEngine;
  config: HostConfig;
  session: SessionState;
  tools?: EngineTool[];
  approvalHandler?: ApprovalHandler;
  signal?: AbortSignal;

  /** Raw engine events (deltas, tool start/end, engine errors). */
  onEvent?: (event: EngineEvent) => void;

  /**
   * Host-level lifecycle events (agent_start, turn_start, turn_end, queue_update,
   * save_point, settled, agent_end, failure). Emitted in addition to raw engine events.
   */
  onLifecycleEvent?: (event: HostLifecycleEvent) => void;

  /**
   * Called at each save_point with the current session state.
   * Use this to persist session messages incrementally (per-turn) instead of
   * only at run completion. The callback is awaited before the next turn begins.
   */
  onSavePoint?: (session: SessionState) => void | Promise<void>;

  /**
   * Optional per-message flush. Called at message_end for assistant messages
   * so that partial progress is persisted even within a single turn.
   * Useful for long-running multi-tool sequences.
   */
  onMessageFlush?: (session: SessionState) => void | Promise<void>;

  /** Retry settings. When absent, retries are disabled. */
  retry?: {
    maxRetries: number;
    baseDelayMs: number;
  };

  /**
   * Called before each turn. Receives TurnBuildContext and returns a full TurnState
   * snapshot (model, provider, tools, activeTools, systemPrompt, thinkingLevel, settings).
   *
   * Use this to dynamically switch model, thinking level, tools, or system prompt
   * based on the previous turn's outcome, or to refresh auth tokens per turn.
   *
   * Replaces the earlier TurnPreparation callback.
   */
  prepareTurn?: PrepareTurnFn;

  /**
   * @deprecated Use prepareTurn (PrepareTurnFn) which returns TurnState instead.
   */
  _prepareTurnLegacy?: (ctx: TurnContext) => TurnPreparation | Promise<TurnPreparation>;

  /**
   * Queues for agent loop semantics.
   * - steeringQueue: messages sent while streaming, injected at next turn start
   * - followUpQueue: messages that trigger another turn after current one completes
   * - nextTurnQueue: messages to inject before the agent fully exits
   */
  steeringQueue?: SteeringMessage[];
  followUpQueue?: FollowUpMessage[];
  nextTurnQueue?: NextTurnMessage[];

  /**
   * How steering messages are consumed from the queue.
   * - "one-at-a-time" (default): consume one message per drain cycle.
   * - "all": consume all pending messages at once.
   */
  steeringMode?: QueueMode;

  /**
   * How follow-up messages are consumed from the queue.
   * - "one-at-a-time" (default): trigger one follow-up turn per cycle.
   * - "all": queue all follow-up messages as a single turn.
   */
  followUpMode?: QueueMode;
}

// ============================================================================
// Deprecated types (backward compatibility)
// ============================================================================

/** @deprecated Replaced by TurnState. Kept for backward compatibility. */
export interface TurnPreparation {
  modelOverride?: string;
  model?: import("@earendil-works/pi-ai").Model<string>;
  provider?: EngineProviderConfig;
  thinkingLevel?: string;
  toolsOverride?: EngineTool[];
  settingsOverride?: Partial<EngineRunSettings>;
}

/** @deprecated Replaced by TurnBuildContext. Kept for backward compatibility. */
export interface TurnContext {
  session: SessionState;
  previousStatus: EngineStepStatus;
  turnIndex: number;
  totalSteps: number;
}

// ============================================================================
// Run result
// ============================================================================

export interface RunResult {
  session: SessionState;
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps" | "context_overflow";
  errorMessage?: string;
}
