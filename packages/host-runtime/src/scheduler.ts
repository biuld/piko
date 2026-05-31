import type {
  EngineEvent,
  EngineInput,
  EngineTool,
  StatelessEngine,
} from "piko-engine-protocol";
import type { ApprovalHandler } from "./approval-controller.js";
import { createApprovalResolution } from "./approval-controller.js";
import type { HostConfig } from "./models/index.js";
import { appendMessages, updateSessionState } from "./session/index.js";
import type { SessionState } from "./session/index.js";

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
  status: "completed" | "aborted" | "error" | "max_steps" | "context_overflow";
  errorMessage?: string;
}

export async function runScheduler(options: SchedulerOptions): Promise<RunResult> {
  const { engine, config, session, approvalHandler, signal, onEvent } = options;
  const { model, provider, settings } = config;

  let currentSession = session;
  let totalSteps = 0;
  const runId = `run-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  currentSession = updateSessionState(currentSession, { runState: "running" });

  while (totalSteps < settings.maxSteps) {
    if (signal?.aborted) {
      return {
        session: updateSessionState(currentSession, { runState: "aborted" }),
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
      tools: [],
      settings,
      pendingApproval: currentSession.pendingApproval,
      engineState: currentSession.engineState,
    };

    const stream = engine.executeStep(input, signal);
    for await (const event of stream) {
      onEvent?.(event);
    }
    const result = await stream.result();

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

    // Handle approval
    if (result.status === "awaiting_approval" && result.pendingApproval && approvalHandler) {
      const decision = await approvalHandler.requestApproval(result.pendingApproval);
      const resolution = createApprovalResolution(runId, stepId, result.pendingApproval, decision, currentSession.messages);
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
  }

  return {
    session: updateSessionState(currentSession, { runState: "completed" }),
    totalSteps,
    status: "max_steps",
  };
}
