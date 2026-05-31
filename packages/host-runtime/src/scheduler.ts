import type { Model } from "@earendil-works/pi-ai";
import type {
  EngineEvent,
  EngineInput,
  EngineProviderConfig,
  EngineRunSettings,
  EngineStepResult,
  EngineTool,
  StatelessEngine,
} from "piko-engine-protocol";
import type { ApprovalHandler } from "./approval-controller.js";
import { createApprovalResolution } from "./approval-controller.js";
import type { HostConfig } from "./models/index.js";
import type { SessionState } from "./session/index.js";
import { addUserMessage, appendMessages, updateSessionState } from "./session/index.js";

// ============================================================================
// Types
// ============================================================================

/** A queued message to inject mid-stream (steering). */
export interface SteeringMessage {
  text: string;
}

/** A queued message to inject after the current turn completes. */
export interface FollowUpMessage {
  text: string;
}

/** A queued message for the next full turn. */
export interface NextTurnMessage {
  text: string;
}

export interface SchedulerOptions {
  engine: StatelessEngine;
  config: HostConfig;
  session: SessionState;
  tools?: EngineTool[];
  approvalHandler?: ApprovalHandler;
  signal?: AbortSignal;
  onEvent?: (event: EngineEvent) => void;

  /** Retry settings. When absent, retries are disabled. */
  retry?: {
    maxRetries: number;
    baseDelayMs: number;
  };

  /**
   * Called before each turn. Can return overrides for the next step.
   * Use this to dynamically switch model, thinking level, or tools.
   */
  prepareTurn?: () => TurnPreparation;

  /**
   * Queues for agent loop semantics.
   * - steeringQueue: messages sent while streaming, injected at next turn start
   * - followUpQueue: messages that trigger another turn after current one completes
   * - nextTurnQueue: messages to inject before the agent fully exits
   */
  steeringQueue?: SteeringMessage[];
  followUpQueue?: FollowUpMessage[];
  nextTurnQueue?: NextTurnMessage[];
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
  } = options;

  // Use the initial config; prepareTurn can override per-step
  const currentModel = config.model;
  const currentProvider = config.provider;
  const currentSettings = config.settings;

  let currentSession = session;
  let totalSteps = 0;
  let consecutiveErrors = 0;
  const runId = `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  currentSession = updateSessionState(currentSession, { runState: "running" });

  /** Drain steering queue into session. Returns true if any messages were injected. */
  function drainSteering(): boolean {
    if (!steeringQueue || steeringQueue.length === 0) return false;
    const steered = steeringQueue.splice(0);
    for (const s of steered) {
      currentSession = addUserMessage(currentSession, s.text);
      onEvent?.({ type: "error", message: `[steer] ${s.text}` });
    }
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
        const prep = prepareTurn();
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
          return {
            session: updateSessionState(currentSession, { runState: "error" }),
            totalSteps,
            status: "error",
            errorMessage: `Step failed after ${maxRetries + 1} attempts: ${lastError}`,
          };
        }
        // Fewer than 3 consecutive — report but continue
        onEvent?.({ type: "error", message: `Step error (non-fatal): ${lastError}` });
        totalSteps++;
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
        }
        continue;
      }

      // ---- Terminal states (break inner loop, check queues) ----
      if (result.status === "completed") {
        // Agent signaled completion — break inner loop to check queues
        completedNaturally = true;
        break;
      }

      if (result.status === "error") {
        return {
          session: updateSessionState(currentSession, { runState: "error" }),
          totalSteps,
          status: "error",
          errorMessage: "Engine step returned error",
        };
      }

      // "continue" — loop inner, but drain steering first (fix #2)
      drainSteering();
    }

    // ---- Check if max steps reached (inner loop condition exhausted, not natural completion) ----
    if (!completedNaturally && totalSteps >= currentSettings.maxSteps) {
      return {
        session: updateSessionState(currentSession, { runState: "completed" }),
        totalSteps,
        status: "max_steps",
      };
    }

    // ---- Check follow-up queue. ----
    if (followUpQueue && followUpQueue.length > 0) {
      const fu = followUpQueue.splice(0, 1)[0]!;
      currentSession = addUserMessage(currentSession, fu.text);
      onEvent?.({ type: "error", message: `[follow-up] ${fu.text}` });
      continue;
    }

    // ---- Check next-turn queue. ----
    if (nextTurnQueue && nextTurnQueue.length > 0) {
      const nt = nextTurnQueue.splice(0, 1)[0]!;
      currentSession = addUserMessage(currentSession, nt.text);
      onEvent?.({ type: "error", message: `[next-turn] ${nt.text}` });
      continue;
    }

    // ---- No more queued messages — true completion ----
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
