import type {
  StatelessEngine,
  EngineInput,
  EngineEvent,
  EngineTool,
} from "piko-engine-protocol";
import type { HostConfig } from "./model-config.js";
import type { SessionState } from "./session-store.js";
import type { ApprovalHandler } from "./approval-controller.js";
import { createApprovalResolution } from "./approval-controller.js";
import { appendMessages, updateSessionState } from "./session-store.js";

export interface SchedulerOptions {
  engine: StatelessEngine;
  config: HostConfig;
  session: SessionState;
  tools?: EngineTool[];
  approvalHandler?: ApprovalHandler;
  signal?: AbortSignal;
  onEvent?: (event: EngineEvent) => void;
}

export interface RunResult {
  session: SessionState;
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps";
  errorMessage?: string;
}

export async function runScheduler(
  options: SchedulerOptions,
): Promise<RunResult> {
  const { engine, config, session, approvalHandler, signal, onEvent } = options;
  const { model, provider, settings } = config;

  let currentSession = session;
  let totalSteps = 0;
  const runId = `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  currentSession = updateSessionState(currentSession, {
    runState: "running",
  });

  while (totalSteps < settings.maxSteps) {
    if (signal?.aborted) {
      currentSession = updateSessionState(currentSession, {
        runState: "aborted",
      });
      return {
        session: currentSession,
        totalSteps,
        status: "aborted",
      };
    }

    const stepId = `step-${totalSteps}-${Date.now()}`;

    const input: EngineInput = {
      runId,
      stepId,
      transcript: currentSession.messages,
      systemPrompt: currentSession.systemPrompt,
      model,
      provider,
      tools: options.tools ?? [],
      settings,
      pendingApproval: currentSession.pendingApproval,
      engineState: currentSession.engineState,
    };

    const stream = engine.executeStep(input, signal);

    for await (const event of stream) {
      onEvent?.(event);
    }

    const result = await stream.result();

    // Append messages from this step
    if (result.appendedMessages.length > 0) {
      currentSession = appendMessages(currentSession, result.appendedMessages);
    }
    currentSession = updateSessionState(currentSession, {
      engineState: result.engineState,
      pendingApproval: result.pendingApproval,
      runState: result.status === "awaiting_approval" ? "awaiting_approval" : "running",
    });

    totalSteps++;

    // Handle approval
    if (result.status === "awaiting_approval" && result.pendingApproval && approvalHandler) {
      const decision = await approvalHandler.requestApproval(
        result.pendingApproval,
      );

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
          currentSession = appendMessages(
            currentSession,
            resumedResult.appendedMessages,
          );
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

    if (result.status === "completed") {
      currentSession = updateSessionState(currentSession, {
        runState: "completed",
        pendingApproval: undefined,
      });
      return {
        session: currentSession,
        totalSteps,
        status: "completed",
      };
    }

    if (result.status === "error") {
      currentSession = updateSessionState(currentSession, {
        runState: "error",
      });
      return {
        session: currentSession,
        totalSteps,
        status: "error",
        errorMessage: "Engine step returned error",
      };
    }

    // result.status === "continue" -> loop again
  }

  return {
    session: updateSessionState(currentSession, {
      runState: "completed",
    }),
    totalSteps,
    status: "max_steps",
  };
}
