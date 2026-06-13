import type { Message } from "./event-stream.js";
import type {
  ModelContinuationState,
  ModelResumeContext,
  ModelRunSettings,
  ModelRuntimeCounters,
  ModelStepInput,
} from "./types.js";

export interface PendingToolCallSnapshot {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  executorTarget?: string;
  executionMode?: "sequential" | "parallel";
  requiresApproval?: boolean;
}

export interface PendingToolSnapshotLike {
  remainingToolCalls: PendingToolCallSnapshot[];
}

export function createReadyContinuationState(
  counters: ModelRuntimeCounters,
): ModelContinuationState {
  return {
    version: 1,
    kind: "ready",
    counters,
  };
}

export function buildPendingToolsContinuationState(
  assistantMessage: Message,
  pendingToolSnapshot: PendingToolSnapshotLike,
  settings: ModelRunSettings,
  counters: ModelRuntimeCounters,
  resumeContext: ModelResumeContext,
): ModelContinuationState {
  const remaining = pendingToolSnapshot.remainingToolCalls;
  return {
    version: 1,
    kind: "pending_tools",
    pendingToolCalls: {
      assistantMessage,
      remainingToolCallIds: remaining.map((tc) => tc.id),
      toolCalls: remaining.map((tc) => ({
        id: tc.id,
        name: tc.name,
        args: tc.arguments,
        executorTarget: tc.executorTarget,
        executionMode: tc.executionMode,
        requiresApproval: tc.requiresApproval,
      })),
      settings,
    },
    resumeContext,
    counters,
  };
}

/**
 * Build a typed ModelContinuationState from a step's outcome.
 */
export function buildContinuationState(
  input: ModelStepInput,
  assistantMessage: Message,
  counters: ModelRuntimeCounters,
  toolResult?: { pendingToolSnapshot?: PendingToolSnapshotLike },
): ModelContinuationState {
  if (toolResult?.pendingToolSnapshot) {
    return buildPendingToolsContinuationState(
      assistantMessage,
      toolResult.pendingToolSnapshot,
      input.settings,
      counters,
      {
        systemPrompt: input.systemPrompt,
        model: input.model,
        provider: input.provider,
        tools: input.tools,
        toolSets: input.toolSets,
        settings: input.settings,
      },
    );
  }

  return createReadyContinuationState(counters);
}

export function extractContinuationStateFromInput(
  input: ModelStepInput,
): ModelContinuationState | undefined {
  const raw = input.engineState;
  if (!raw) return undefined;

  if (
    typeof raw === "object" &&
    raw !== null &&
    "version" in raw &&
    (raw as ModelContinuationState).version === 1
  ) {
    return raw as ModelContinuationState;
  }

  return undefined;
}

export function getOrCreateCounters(input: ModelStepInput): ModelRuntimeCounters {
  const prev = extractContinuationStateFromInput(input);
  return (
    prev?.counters ?? {
      modelCalls: 0,
      toolCalls: 0,
      consecutiveErrors: 0,
      startedAt: Date.now(),
    }
  );
}
