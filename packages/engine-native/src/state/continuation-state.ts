import type {
  EngineContinuationState,
  EngineInput,
  EngineRunSettings,
  EngineRuntimeCounters,
  Message,
} from "piko-protocol";

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
  counters: EngineRuntimeCounters,
): EngineContinuationState {
  return {
    version: 1,
    kind: "ready",
    counters,
  };
}

export function buildPendingToolsContinuationState(
  assistantMessage: Message,
  pendingToolSnapshot: PendingToolSnapshotLike,
  settings: EngineRunSettings,
  counters: EngineRuntimeCounters,
): EngineContinuationState {
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
    counters,
  };
}

/**
 * Build a typed EngineContinuationState from a step's outcome.
 */
export function buildContinuationState(
  input: EngineInput,
  assistantMessage: Message,
  counters: EngineRuntimeCounters,
  toolResult?: { pendingToolSnapshot?: PendingToolSnapshotLike },
): EngineContinuationState {
  if (toolResult?.pendingToolSnapshot) {
    return buildPendingToolsContinuationState(
      assistantMessage,
      toolResult.pendingToolSnapshot,
      input.settings,
      counters,
    );
  }

  return createReadyContinuationState(counters);
}

export function extractContinuationStateFromInput(
  input: EngineInput,
): EngineContinuationState | undefined {
  const raw = input.engineState;
  if (!raw) return undefined;

  if (
    typeof raw === "object" &&
    raw !== null &&
    "version" in raw &&
    (raw as EngineContinuationState).version === 1
  ) {
    return raw as EngineContinuationState;
  }

  return undefined;
}

export function getOrCreateCounters(input: EngineInput): EngineRuntimeCounters {
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
