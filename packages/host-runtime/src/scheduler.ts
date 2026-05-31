import type {
  EngineEvent,
  EngineInput,
  EngineRunSettings,
  EngineStepResult,
  EngineTool,
  StatelessEngine,
} from "piko-engine-protocol";
import type { ApprovalHandler } from "./approval-controller.js";
import { createApprovalResolution } from "./approval-controller.js";
import type { HostConfig } from "./models/index.js";
import { appendMessages, updateSessionState } from "./session/index.js";
import type { SessionState } from "./session/index.js";

// ============================================================================
// Types
// ============================================================================

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
}

/** Overrides that can be applied per-turn. */
export interface TurnPreparation {
  /** Override model for this turn (provider/model string). */
  modelOverride?: string;
  /** Override thinking level for this turn. */
  thinkingLevel?: string;
  /** Override tools for this turn. */
  toolsOverride?: EngineTool[];
  /** Additional settings overrides. */
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
  } = options;
  const { model, provider, settings } = config;

  let currentSession = session;
  let totalSteps = 0;
  let consecutiveErrors = 0;
  const runId = `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  currentSession = updateSessionState(currentSession, { runState: "running" });

  // ---- Main loop ----
  while (totalSteps < settings.maxSteps) {
    if (signal?.aborted) {
      return {
        session: updateSessionState(currentSession, { runState: "aborted" }),
        totalSteps,
        status: "aborted",
      };
    }

    const stepId = `step-${totalSteps}-${Date.now()}`;

    // ---- Prepare turn (dynamic model/thinking/tools) ----
    let effectiveModel = model;
    let effectiveProvider = provider;
    let effectiveSettings = settings;
    let effectiveTools = options.tools ?? [];

    if (prepareTurn) {
      const prep = prepareTurn();
      if (prep.modelOverride) {
        // Parse "provider/model" format
        const parts = prep.modelOverride.split("/");
        if (parts.length === 2) {
          effectiveProvider = { ...provider, /* model change only */ };
          effectiveModel = { ...model, provider: parts[0], id: parts[1] };
        }
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
          const delay = retry!.baseDelayMs * Math.pow(2, attempt);
          onEvent?.({ type: "error", message: `Step failed, retrying in ${delay}ms (attempt ${attempt + 1}/${maxRetries})` });
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
        runId, stepId, result.pendingApproval, decision, currentSession.messages,
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

    // ---- Terminal states ----
    if (result.status === "completed") {
      return {
        session: updateSessionState(currentSession, { runState: "completed", pendingApproval: undefined }),
        totalSteps,
        status: "completed",
      };
    }

    if (result.status === "error") {
      return {
        session: updateSessionState(currentSession, { runState: "error" }),
        totalSteps,
        status: "error",
        errorMessage: "Engine step returned error",
      };
    }

    // "continue" — loop
  }

  return {
    session: updateSessionState(currentSession, { runState: "completed" }),
    totalSteps,
    status: "max_steps",
  };
}
