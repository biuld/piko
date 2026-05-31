import type { EngineInput, EngineStepResult, StatelessEngine } from "piko-engine-protocol";
import { createApprovalResolution } from "../approval-controller.js";
import type { HostConfig } from "../models/index.js";
import type { SessionState } from "../session/index.js";
import { appendMessages, updateSessionState } from "../session/index.js";
import type { TurnBuildContext, TurnState } from "../turn-state.js";
import { createEngineEventProcessor } from "./engine-events.js";
import { createLifecycleEmitter, emitFailureMessage, emitSavePoint } from "./lifecycle.js";
import { drainFollowUp, drainNextTurn, drainSteering } from "./steering.js";
import type { RunResult, SchedulerOptions } from "./types.js";

// ============================================================================
// buildDefaultTurnState
// ============================================================================

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

// ============================================================================
// runScheduler
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

  const emitLifecycle = createLifecycleEmitter(onLifecycleEvent);
  const maxSteps = config.settings.maxSteps;

  let currentSession = session;
  let totalSteps = 0;
  let consecutiveErrors = 0;
  let turnIndex = 0;
  const runId = `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  currentSession = updateSessionState(currentSession, { runState: "running" });

  // ---- agent_start ----
  emitLifecycle({ type: "agent_start", runId });

  // ---- Outer loop: continues when follow-up / next-turn messages are queued ----
  while (true) {
    const drained = drainSteering(
      emitLifecycle,
      currentSession,
      steeringQueue,
      followUpQueue,
      nextTurnQueue,
      steeringMode,
    );
    currentSession = drained.session;

    // ---- Inner step loop ----
    let completedNaturally = false;
    while (totalSteps < maxSteps) {
      if (signal?.aborted) {
        emitFailureMessage(emitLifecycle, "Run aborted");
        emitLifecycle({ type: "failure", error: "Run aborted", aborted: true });
        emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
        return {
          session: updateSessionState(currentSession, { runState: "aborted" }),
          totalSteps,
          status: "aborted",
        };
      }

      const stepId = `step-${totalSteps}-${Date.now()}`;

      // ---- Prepare turn ----
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

      // ---- Build engine input ----
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

      emitLifecycle({ type: "turn_start", turnIndex });

      for (let attempt = 0; attempt <= maxRetries; attempt++) {
        try {
          const { process } = createEngineEventProcessor(onEvent, emitLifecycle);
          const stream = engine.executeStep(input, signal);
          for await (const event of stream) {
            process(event);
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
        if (consecutiveErrors >= 3) {
          await emitSavePoint(emitLifecycle, onSavePoint, currentSession);
          const errorMsg = `Step failed after ${maxRetries + 1} attempts: ${lastError}`;
          emitFailureMessage(emitLifecycle, errorMsg);
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
        onEvent?.({ type: "error", message: `Step error (non-fatal): ${lastError}` });
        emitLifecycle({ type: "turn_end", turnIndex });
        await emitSavePoint(emitLifecycle, onSavePoint, currentSession);
        totalSteps++;
        turnIndex++;
        continue;
      }

      // ---- Check for context overflow ----
      if (result.status === "error") {
        const isOverflow = result.appendedMessages.some(
          (m) =>
            (typeof m.content === "string" && m.content.toLowerCase().includes("context")) ||
            (typeof m.content === "string" && m.content.toLowerCase().includes("token")),
        );
        if (isOverflow) {
          await emitSavePoint(emitLifecycle, onSavePoint, currentSession);
          emitFailureMessage(emitLifecycle, "Context overflow — compaction needed");
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

      // ---- Append messages ----
      if (result.appendedMessages.length > 0) {
        currentSession = appendMessages(currentSession, result.appendedMessages);
        if (onMessageFlush) {
          try {
            await onMessageFlush(currentSession);
          } catch {
            /* non-fatal */
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
            if (onMessageFlush) {
              try {
                await onMessageFlush(currentSession);
              } catch {
                /* non-fatal */
              }
            }
          }
          currentSession = updateSessionState(currentSession, {
            engineState: resumedResult.engineState,
            pendingApproval: resumedResult.pendingApproval,
            runState: resumedResult.status === "completed" ? "completed" : "running",
          });
          totalSteps++;
          result = resumedResult;
        }
      }

      // ---- Terminal states ----
      if (result.status === "completed") {
        emitLifecycle({ type: "turn_end", turnIndex });
        await emitSavePoint(emitLifecycle, onSavePoint, currentSession);
        turnIndex++;
        completedNaturally = true;
        break;
      }
      if (result.status === "error") {
        await emitSavePoint(emitLifecycle, onSavePoint, currentSession);
        emitFailureMessage(emitLifecycle, "Engine step returned error");
        emitLifecycle({ type: "failure", error: "Engine step returned error", aborted: false });
        emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
        return {
          session: updateSessionState(currentSession, { runState: "error" }),
          totalSteps,
          status: "error",
          errorMessage: "Engine step returned error",
        };
      }

      // "continue" — next inner iteration
      emitLifecycle({ type: "turn_end", turnIndex });
      await emitSavePoint(emitLifecycle, onSavePoint, currentSession);
      turnIndex++;
      // Drain steering before next turn
      const sd = drainSteering(
        emitLifecycle,
        currentSession,
        steeringQueue,
        followUpQueue,
        nextTurnQueue,
        steeringMode,
      );
      currentSession = sd.session;
    }

    // ---- Max steps reached (not natural completion) ----
    if (!completedNaturally && totalSteps >= maxSteps) {
      emitLifecycle({ type: "settled", nextTurnCount: nextTurnQueue?.length ?? 0 });
      emitLifecycle({ type: "agent_end", status: "max_steps", totalSteps });
      return {
        session: updateSessionState(currentSession, { runState: "completed" }),
        totalSteps,
        status: "max_steps",
      };
    }

    // ---- Check follow-up queue ----
    const fu = drainFollowUp(
      emitLifecycle,
      currentSession,
      followUpQueue,
      nextTurnQueue,
      followUpMode,
    );
    currentSession = fu.session;
    if (fu.hasMore) continue;

    // ---- Check next-turn queue ----
    const nt = drainNextTurn(emitLifecycle, currentSession, nextTurnQueue, followUpQueue);
    currentSession = nt.session;
    if (nt.drained) continue;

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
