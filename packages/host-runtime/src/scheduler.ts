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
import type { PrepareTurnFn, TurnBuildContext, TurnState } from "./turn-state.js";

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

/** @deprecated Replaced by TurnState. Kept for backward compatibility. */
export interface TurnPreparation {
  modelOverride?: string;
  model?: Model<string>;
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

/**
 * Build a default TurnState from the initial config and current session.
 * Used when no prepareTurn callback is provided.
 */
export function buildDefaultTurnState(
  config: HostConfig,
  session: SessionState,
  turnIndex: number,
): TurnState {
  const tools = config.tools ?? [];
  return {
    turnIndex,
    messages: session.messages,
    systemPrompt: session.systemPrompt,
    model: config.model,
    provider: config.provider,
    thinkingLevel: config.settings.thinkingLevel,
    allTools: tools,
    activeTools: tools,
    settings: config.settings,
  };
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
    onMessageFlush,
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
  const maxSteps = config.settings.maxSteps;

  let currentSession = session;
  let totalSteps = 0;
  let consecutiveErrors = 0;
  let turnIndex = 0;
  const runId = `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  currentSession = updateSessionState(currentSession, { runState: "running" });

  // ---- agent_start ----
  emitLifecycle({ type: "agent_start", runId });

  /**
   * Process a raw engine event: pass to onEvent consumer, then map to host lifecycle.
   * Tracks per-step message state so message_start is emitted before the first delta
   * of each new assistant message.
   * Also tracks tool names by ID (since tool_call_end does not carry the tool name).
   *
   * Guarantees:
   * - Every message_end is preceded by a matching message_start.
   * - All events within a single step share the same messageId (no duplicate starts).
   * - Tool-only responses (no message_delta) correctly emit message_start before tool calls.
   */
  let currentMessageId: string | null = null;
  const toolNameById = new Map<string, string>();

  /**
   * Get or create the step-level message ID.
   * All lifecycle events within a step use this same ID, ensuring balanced start/end pairs.
   */
  function stepMessageId(): string {
    if (!currentMessageId) {
      currentMessageId = `msg-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      emitLifecycle({ type: "message_start", messageId: currentMessageId, role: "assistant" });
    }
    return currentMessageId;
  }

  function processEngineEvent(event: EngineEvent): void {
    // Always forward raw engine events to the consumer (TUI, etc.)
    onEvent?.(event);

    // Map engine events to host lifecycle events
    switch (event.type) {
      case "message_delta": {
        const msgId = event.messageId;
        // Use the engine-provided messageId if available, otherwise step-level
        if (!currentMessageId || currentMessageId !== msgId) {
          currentMessageId = msgId;
          emitLifecycle({ type: "message_start", messageId: msgId, role: "assistant" });
        }
        emitLifecycle({
          type: "message_update",
          messageId: msgId,
          delta: event.delta,
          isThinking: false,
        });
        break;
      }
      case "thinking_delta": {
        const msgId = event.messageId;
        if (!currentMessageId || currentMessageId !== msgId) {
          currentMessageId = msgId;
          emitLifecycle({ type: "message_start", messageId: msgId, role: "assistant" });
        }
        emitLifecycle({
          type: "message_update",
          messageId: msgId,
          delta: event.delta,
          isThinking: true,
        });
        break;
      }
      case "message_end": {
        // Ensure a message_start was emitted (for tool-only responses).
        // Uses the step-level ID to match the tool_call_start IDs.
        stepMessageId();

        // Extract text content from the engine message for the lifecycle descriptor
        const msgContent =
          typeof event.message.content === "string"
            ? event.message.content
            : Array.isArray(event.message.content)
              ? event.message.content
                  .filter((c): c is { type: "text"; text: string } => c.type === "text")
                  .map((c) => c.text)
                  .join("")
              : "";
        emitLifecycle({
          type: "message_end",
          message: { role: "assistant", content: msgContent },
        });
        // Reset for next potential message in the same step
        currentMessageId = null;
        break;
      }
      case "tool_call_start":
        // Ensure message_start exists (uses step-level ID shared with message_end).
        stepMessageId();
        toolNameById.set(event.id, event.name);
        emitLifecycle({
          type: "tool_execution_start",
          toolCallId: event.id,
          toolName: event.name,
          args: event.args,
        });
        break;
      case "tool_call_end":
        emitLifecycle({
          type: "tool_execution_end",
          toolCallId: event.id,
          toolName: toolNameById.get(event.id) ?? event.id,
          result: event.result,
          isError: event.isError,
        });
        break;
      // step_start, step_end, approval_requested, error — no host lifecycle mapping needed
    }
  }

  /** Emit a failure message lifecycle: message_start → message_end for a synthetic assistant message. */
  function emitFailureMessage(errorText: string): void {
    const failureId = `failure-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    emitLifecycle({ type: "message_start", messageId: failureId, role: "assistant" });
    emitLifecycle({
      type: "message_end",
      message: { role: "assistant", content: errorText, messageId: failureId },
    });
  }

  /** Emit lifecycle for a user message being injected (steering / follow-up / next-turn). */
  function emitUserMessageLifecycle(text: string, source: string): void {
    const msgId = `user-${source}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    emitLifecycle({ type: "message_start", messageId: msgId, role: "user" });
    emitLifecycle({
      type: "message_end",
      message: { role: "user", content: text, messageId: msgId },
    });
  }

  /** Emit queue_update with current queue sizes and optional message previews. */
  function emitQueueUpdate(): void {
    const MAX_PREVIEW_LEN = 80;
    const steerPreview = steeringQueue?.[0]?.text?.slice(0, MAX_PREVIEW_LEN);
    const followUpPreview = followUpQueue?.[0]?.text?.slice(0, MAX_PREVIEW_LEN);
    const nextTurnPreview = nextTurnQueue?.[0]?.text?.slice(0, MAX_PREVIEW_LEN);
    emitLifecycle({
      type: "queue_update",
      steerCount: steeringQueue?.length ?? 0,
      followUpCount: followUpQueue?.length ?? 0,
      nextTurnCount: nextTurnQueue?.length ?? 0,
      steerPreview,
      followUpPreview,
      nextTurnPreview,
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
      emitUserMessageLifecycle(s.text, "steer");
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
    while (totalSteps < maxSteps) {
      if (signal?.aborted) {
        emitFailureMessage("Run aborted");
        emitLifecycle({ type: "failure", error: "Run aborted", aborted: true });
        emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
        return {
          session: updateSessionState(currentSession, { runState: "aborted" }),
          totalSteps,
          status: "aborted",
        };
      }

      const stepId = `step-${totalSteps}-${Date.now()}`;

      // ---- Prepare turn (build full TurnState snapshot) ----
      let turnState: TurnState;
      if (prepareTurn) {
        const ctx: TurnBuildContext = {
          session: currentSession,
          previousStatus:
            currentSession.runState === "awaiting_approval" ? "awaiting_approval" : "continue",
          turnIndex,
          totalSteps,
        };
        turnState = await prepareTurn(ctx);
      } else {
        turnState = buildDefaultTurnState(config, currentSession, turnIndex);
      }

      // ---- Build engine input from turn state ----
      // Pass undefined (not []) when no tools are configured, so the engine
      // uses its built-in defaults. Only pass tools when allTools is non-empty
      // (meaning they were explicitly configured and filtering may apply).
      const effectiveTools = turnState.allTools.length > 0 ? turnState.activeTools : undefined;

      const input: EngineInput = {
        runId,
        stepId,
        transcript: turnState.messages,
        systemPrompt: turnState.systemPrompt,
        model: turnState.model,
        provider: turnState.provider,
        tools: effectiveTools,
        settings: turnState.settings,
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
          currentMessageId = null; // Reset message tracking per step
          const stream = engine.executeStep(input, signal);
          for await (const event of stream) {
            processEngineEvent(event);
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
          const errorMsg = `Step failed after ${maxRetries + 1} attempts: ${lastError}`;
          emitFailureMessage(errorMsg);
          emitLifecycle({
            type: "failure",
            error: errorMsg,
            aborted: false,
          });
          emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
          return {
            session: updateSessionState(currentSession, { runState: "error" }),
            totalSteps,
            status: "error",
            errorMessage: errorMsg,
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
          emitFailureMessage("Context overflow — compaction needed");
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
        // Per-message flush: persist incremental progress within a turn
        if (onMessageFlush) {
          try {
            await onMessageFlush(currentSession);
          } catch {
            // Flush failure is non-fatal — next save_point will retry
          }
        }
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
            // Per-message flush: persist incremental progress within a turn
            if (onMessageFlush) {
              try {
                await onMessageFlush(currentSession);
              } catch {
                // Flush failure is non-fatal
              }
            }
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
        emitFailureMessage("Engine step returned error");
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
    if (!completedNaturally && totalSteps >= maxSteps) {
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
        emitUserMessageLifecycle(msg.text, "follow-up");
      }
      emitQueueUpdate();
      continue;
    }

    // ---- Check next-turn queue. ----
    if (nextTurnQueue && nextTurnQueue.length > 0) {
      const nt = nextTurnQueue.splice(0, 1)[0]!;
      currentSession = addUserMessage(currentSession, nt.text, nt.images);
      emitUserMessageLifecycle(nt.text, "next-turn");
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
