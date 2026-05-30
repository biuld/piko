import type {
  EngineEvent,
  EngineInput,
  EngineTool,
  Message,
  StatelessEngine,
} from "piko-engine-protocol";
import type { ApprovalHandler } from "./approval-controller.js";
import { createApprovalResolution } from "./approval-controller.js";
import type { HostConfig } from "./model-config.js";
import type { SessionState } from "./session/index.js";

// ============================================================================
// Context compaction helpers
// ============================================================================

/** Rough token estimation: 1 token ≈ 4 characters */
function estimateTokens(messages: Message[]): number {
  let total = 0;
  for (const msg of messages) {
    const content = typeof msg.content === "string" ? msg.content : JSON.stringify(msg.content);
    total += Math.ceil(content.length / 4);
  }
  return total;
}

/**
 * Compact the session by summarizing old messages.
 * Keeps the most recent messages (below threshold) and replaces older ones with a compaction marker.
 */
function compactSession(
  session: SessionState,
  contextWindow: number,
  threshold: number,
): SessionState {
  const messages = session.messages;
  if (messages.length <= 4) return session; // Too few to compact

  // Find the cutoff: keep enough recent messages to stay under threshold
  const targetTokens = Math.floor(contextWindow * threshold * 0.5); // compact to 50% of threshold
  let recentTokens = 0;
  let cutoffIdx = messages.length;

  for (let i = messages.length - 1; i >= 0; i--) {
    const content =
      typeof messages[i].content === "string"
        ? messages[i].content
        : JSON.stringify(messages[i].content);
    recentTokens += Math.ceil(content.length / 4);
    if (recentTokens > targetTokens) {
      cutoffIdx = i + 1;
      break;
    }
  }

  if (cutoffIdx <= 0 || cutoffIdx >= messages.length) return session;

  // Replace older messages with a single compaction summary message
  const keptMessages = messages.slice(cutoffIdx);
  const summaryMsg = {
    role: "assistant" as const,
    content: `[Context compacted: ${messages.length - keptMessages.length} earlier messages summarized for space]`,
  } as unknown as Message;

  return {
    ...session,
    messages: [summaryMsg, ...keptMessages],
  };
}

import { appendMessages, updateSessionState } from "./session/index.js";

export interface SchedulerOptions {
  engine: StatelessEngine;
  config: HostConfig;
  session: SessionState;
  tools?: EngineTool[];
  approvalHandler?: ApprovalHandler;
  signal?: AbortSignal;
  onEvent?: (event: EngineEvent) => void;
  /** Context compaction threshold (0-1). Triggers when usage exceeds this fraction of context window. Default: 0.7 */
  compactThreshold?: number;
}

export interface RunResult {
  session: SessionState;
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps";
  errorMessage?: string;
}

export async function runScheduler(options: SchedulerOptions): Promise<RunResult> {
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

    // Check context usage and compact if needed
    const threshold = options.compactThreshold ?? 0.7;
    const contextWindow = (model as { contextWindow?: number }).contextWindow ?? 200_000;
    const estimatedTokens = estimateTokens(currentSession.messages);
    if (estimatedTokens > contextWindow * threshold) {
      currentSession = compactSession(currentSession, contextWindow, threshold);
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
