import type { Model } from "@earendil-works/pi-ai";
import type {
  EngineEvent,
  EngineInput,
  EngineProviderConfig,
  EngineRunSettings,
  EngineStepResult,
  EngineStepStatus,
  EngineTool,
  ImageContent,
  StatelessEngine,
} from "piko-engine-protocol";
import type { ApprovalHandler } from "./approval-controller.js";
import { createApprovalResolution } from "./approval-controller.js";
import type { HostLifecycleEvent } from "./host/lifecycle-events.js";
import type { HostConfig } from "./models/index.js";
import type { SessionState } from "./session/index.js";
import { addUserMessage, appendMessages, updateSessionState } from "./session/index.js";

// ============================================================================
// Types
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

  /** Retry settings. When absent, retries are disabled. */
  retry?: {
    maxRetries: number;
    baseDelayMs: number;
  };

  /**
   * Called before each turn. Receives TurnContext with previous step info.
   * Can return overrides for the next step synchronously or asynchronously.
   * Use this to dynamically switch model, thinking level, or tools based on
   * the previous turn's outcome.
   */
  prepareTurn?: (ctx: TurnContext) => TurnPreparation | Promise<TurnPreparation>;

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

/** Overrides that can be applied per-turn. */
export interface TurnPreparation {
  /** Override model for this turn (provider/model string). */
  modelOverride?: string;
  /** Full model object override (takes precedence over modelOverride string). */
  model?: Model<string>;
  /** Full provider config override (takes precedence). */
  provider?: EngineProviderConfig;
  /** Override thinking level for this turn. */
  thinkingLevel?: string;
  /** Override tools for this turn. */
  toolsOverride?: EngineTool[];
  /** Additional settings overrides (merged on top of existing). */
  settingsOverride?: Partial<EngineRunSettings>;
}

/**
 * Context passed to prepareTurn before each engine step.
 * Enables dynamic adjustment of model, thinking, tools, and settings
 * based on the previous turn's outcome.
 */
export interface TurnContext {
  /** The session state after the previous step (messages, systemPrompt, etc.). */
  session: SessionState;
  /** The status of the most recently completed step. */
  previousStatus: EngineStepStatus;
  /** 0-based index of the upcoming turn. */
  turnIndex: number;
  /** Total engine steps executed so far. */
  totalSteps: number;
}

export interface RunResult {
  session: SessionState;
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps" | "context_overflow";
  errorMessage?: string;
}

// ============================================================================
// Scheduler
// ============================================================================

export async function runScheduler(options: SchedulerOptions): Promise<RunResult> {
  const {
    engine,
    config,
    session,
    approvalHandler,
    signal,
    onEvent,
    retry,
    prepareTurn,
    steeringQueue,
    followUpQueue,
    nextTurnQueue,
    onLifecycleEvent,
    steeringMode = "all",
    followUpMode = "one-at-a-time",
    onSavePoint,
  } = options;

  // Shorthand for emitting lifecycle events
  const life = onLifecycleEvent;

  /** Safely emit a lifecycle event, swallowing errors from the callback. */
  function emitLifecycle(event: HostLifecycleEvent): void {
    try {
      const result = life?.(event);
      // If the callback returns a thenable, catch rejections to avoid unhandled rejections
      if (result && typeof (result as unknown as Promise<void>).catch === "function") {
        (result as unknown as Promise<void>).catch(() => {});
      }
    } catch {
      // Swallow sync errors from lifecycle callbacks
    }
  }

  // Use the initial config; prepareTurn can override per-step
  const currentModel = config.model;
  const currentProvider = config.provider;
  const currentSettings = config.settings;

  let currentSession = session;
  let totalSteps = 0;
  let consecutiveErrors = 0;
  let turnIndex = 0;
  const runId = `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  currentSession = updateSessionState(currentSession, { runState: "running" });

  // ---- agent_start ----
  emitLifecycle({ type: "agent_start", runId });

  /** Emit queue_update with current queue sizes. */
  function emitQueueUpdate(): void {
    emitLifecycle({
      type: "queue_update",
      steerCount: steeringQueue?.length ?? 0,
      followUpCount: followUpQueue?.length ?? 0,
      nextTurnCount: nextTurnQueue?.length ?? 0,
    });
  }

  /**
   * Emit save_point lifecycle event and flush pending session writes.
   * Called at turn boundaries so messages are persisted incrementally.
   */
  async function emitSavePoint(): Promise<void> {
    emitLifecycle({ type: "save_point", hadPendingWrites: true });
    if (onSavePoint) {
      await onSavePoint(currentSession);
    }
  }

  /** Drain steering queue into session. Returns true if any messages were injected. */
  function drainSteering(): boolean {
    if (!steeringQueue || steeringQueue.length === 0) return false;
    const steered = steeringMode === "all" ? steeringQueue.splice(0) : steeringQueue.splice(0, 1);
    for (const s of steered) {
      currentSession = addUserMessage(currentSession, s.text, s.images);
      onEvent?.({ type: "error", message: `[steer] ${s.text}` });
    }
    emitQueueUpdate();
    return true;
  }

  // ---- Outer loop: continues when follow-up / next-turn messages are queued ----
  while (true) {
    // Drain steering once at outer loop entry
    drainSteering();

    // ---- Inner step loop ----
    let completedNaturally = false;
    while (totalSteps < currentSettings.maxSteps) {
      if (signal?.aborted) {
        emitLifecycle({ type: "failure", error: "Run aborted", aborted: true });
        emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
        return {
          session: updateSessionState(currentSession, { runState: "aborted" }),
          totalSteps,
          status: "aborted",
        };
      }

      const stepId = `step-${totalSteps}-${Date.now()}`;

      // ---- Prepare turn (dynamic model/thinking/tools) ----
      let effectiveModel = currentModel;
      let effectiveProvider = currentProvider;
      let effectiveSettings = currentSettings;
      let effectiveTools = options.tools ?? [];

      if (prepareTurn) {
        const ctx: TurnContext = {
          session: currentSession,
          previousStatus:
            currentSession.runState === "awaiting_approval" ? "awaiting_approval" : "continue",
          turnIndex,
          totalSteps,
        };
        const prep = await prepareTurn(ctx);
        // Full model/provider objects take precedence (fix #1)
        if (prep.model) {
          effectiveModel = prep.model;
        } else if (prep.modelOverride) {
          const parts = prep.modelOverride.split("/");
          if (parts.length === 2) {
            effectiveModel = { ...currentModel, provider: parts[0], id: parts[1] };
          }
        }
        if (prep.provider) {
          effectiveProvider = prep.provider;
        }
        if (prep.thinkingLevel) {
          effectiveSettings = {
            ...effectiveSettings,
            thinkingLevel: prep.thinkingLevel as EngineRunSettings["thinkingLevel"],
          };
        }
        if (prep.settingsOverride) {
          effectiveSettings = { ...effectiveSettings, ...prep.settingsOverride };
        }
        if (prep.toolsOverride) {
          effectiveTools = prep.toolsOverride;
        }
      }

      // ---- Build engine input ----
      const input: EngineInput = {
        runId,
        stepId,
        transcript: currentSession.messages,
        systemPrompt: currentSession.systemPrompt,
        model: effectiveModel,
        provider: effectiveProvider,
        tools: effectiveTools,
        settings: effectiveSettings,
        pendingApproval: currentSession.pendingApproval,
        engineState: currentSession.engineState,
      };

      // ---- Execute step (with optional retry) ----
      const maxRetries = retry?.maxRetries ?? 0;
      let lastError: string | undefined;
      let result: EngineStepResult | null = null;

      // ---- turn_start (before engine step) ----
      emitLifecycle({ type: "turn_start", turnIndex });

      for (let attempt = 0; attempt <= maxRetries; attempt++) {
        try {
          const stream = engine.executeStep(input, signal);
          for await (const event of stream) {
            onEvent?.(event);
          }
          result = await stream.result();
          consecutiveErrors = 0;
          break;
        } catch (err) {
          lastError = err instanceof Error ? err.message : String(err);
          consecutiveErrors++;

          if (attempt < maxRetries) {
            const delay = retry!.baseDelayMs * 2 ** attempt;
            onEvent?.({
              type: "error",
              message: `Step failed, retrying in ${delay}ms (attempt ${attempt + 1}/${maxRetries})`,
            });
            await new Promise((resolve) => setTimeout(resolve, delay));

            if (signal?.aborted) break;
          }
        }
      }

      if (!result) {
        // All retries exhausted
        if (consecutiveErrors >= 3) {
          emitLifecycle({ type: "turn_end", turnIndex });
          await emitSavePoint();
          emitLifecycle({
            type: "failure",
            error: `Step failed after ${maxRetries + 1} attempts: ${lastError}`,
            aborted: false,
          });
          emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
          return {
            session: updateSessionState(currentSession, { runState: "error" }),
            totalSteps,
            status: "error",
            errorMessage: `Step failed after ${maxRetries + 1} attempts: ${lastError}`,
          };
        }
        // Fewer than 3 consecutive — report but continue
        onEvent?.({ type: "error", message: `Step error (non-fatal): ${lastError}` });
        emitLifecycle({ type: "turn_end", turnIndex });
        await emitSavePoint();
        totalSteps++;
        turnIndex++;
        continue;
      }

      // ---- Handle result ----

      // Check for context overflow
      if (result.status === "error") {
        const isOverflow = result.appendedMessages.some(
          (m) =>
            (typeof m.content === "string" && m.content.toLowerCase().includes("context")) ||
            (typeof m.content === "string" && m.content.toLowerCase().includes("token")),
        );
        if (isOverflow) {
          emitLifecycle({ type: "turn_end", turnIndex });
          await emitSavePoint();
          emitLifecycle({
            type: "failure",
            error: "Context overflow — compaction needed",
            aborted: false,
          });
          emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
          return {
            session: currentSession,
            totalSteps,
            status: "context_overflow",
            errorMessage: "Context overflow — compaction needed",
          };
        }
      }

      // Append messages
      if (result.appendedMessages.length > 0) {
        currentSession = appendMessages(currentSession, result.appendedMessages);
      }
      currentSession = updateSessionState(currentSession, {
        engineState: result.engineState,
        pendingApproval: result.pendingApproval,
        runState: result.status === "awaiting_approval" ? "awaiting_approval" : "running",
      });
      totalSteps++;

      // ---- Handle approval ----
      if (result.status === "awaiting_approval" && result.pendingApproval && approvalHandler) {
        const decision = await approvalHandler.requestApproval(result.pendingApproval);
        const resolution = createApprovalResolution(
          runId,
          stepId,
          result.pendingApproval,
          decision,
          currentSession.messages,
        );

        if (engine.resolveApproval) {
          const resumedResult = await engine.resolveApproval(resolution, signal);
          if (resumedResult.appendedMessages.length > 0) {
            currentSession = appendMessages(currentSession, resumedResult.appendedMessages);
          }
          currentSession = updateSessionState(currentSession, {
            engineState: resumedResult.engineState,
            pendingApproval: resumedResult.pendingApproval,
            runState: resumedResult.status === "completed" ? "completed" : "running",
          });
          totalSteps++;
          // Override result so terminal state checks below use the resolved outcome
          result = resumedResult;
        }
        // Fall through to turn_end and terminal state handling
      }

      // ---- Terminal states (break inner loop, check queues) ----
      if (result.status === "completed") {
        // Agent signaled completion — break inner loop to check queues
        emitLifecycle({ type: "turn_end", turnIndex });
        await emitSavePoint();
        turnIndex++;
        completedNaturally = true;
        break;
      }

      if (result.status === "error") {
        emitLifecycle({ type: "turn_end", turnIndex });
        await emitSavePoint();
        emitLifecycle({ type: "failure", error: "Engine step returned error", aborted: false });
        emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
        return {
          session: updateSessionState(currentSession, { runState: "error" }),
          totalSteps,
          status: "error",
          errorMessage: "Engine step returned error",
        };
      }

      // "continue" — emit turn_end, then loop inner (drain steering first)
      emitLifecycle({ type: "turn_end", turnIndex });
      await emitSavePoint();
      turnIndex++;
      drainSteering();
    }

    // ---- Check if max steps reached (inner loop condition exhausted, not natural completion) ----
    if (!completedNaturally && totalSteps >= currentSettings.maxSteps) {
      emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
      emitLifecycle({ type: "agent_end", status: "max_steps", totalSteps });
      return {
        session: updateSessionState(currentSession, { runState: "completed" }),
        totalSteps,
        status: "max_steps",
      };
    }

    // ---- Check follow-up queue. ----
    if (followUpQueue && followUpQueue.length > 0) {
      const drained = followUpMode === "all" ? followUpQueue.splice(0) : followUpQueue.splice(0, 1);
      for (const msg of drained) {
        currentSession = addUserMessage(currentSession, msg.text, msg.images);
        onEvent?.({ type: "error", message: `[follow-up] ${msg.text}` });
      }
      emitQueueUpdate();
      continue;
    }

    // ---- Check next-turn queue. ----
    if (nextTurnQueue && nextTurnQueue.length > 0) {
      const nt = nextTurnQueue.splice(0, 1)[0]!;
      currentSession = addUserMessage(currentSession, nt.text, nt.images);
      onEvent?.({ type: "error", message: `[next-turn] ${nt.text}` });
      emitQueueUpdate();
      continue;
    }

    // ---- No more queued messages — true completion ----
    emitLifecycle({ type: "settled", nextTurnCount: 0 });
    emitLifecycle({ type: "agent_end", status: "completed", totalSteps });
    return {
      session: updateSessionState(currentSession, {
        runState: "completed",
        pendingApproval: undefined,
      }),
      totalSteps,
      status: "completed",
    };
  }
}
