import type {
  EngineApprovalResolution,
  EngineContinuationState,
  PendingApprovalState,
} from "piko-engine-protocol";

export interface ApprovalContext {
  requestId: string;
  kind: string;
  details: unknown;
}

export function createPendingApproval(
  approval: ApprovalContext,
  continuationState?: EngineContinuationState,
): PendingApprovalState {
  return {
    requestId: approval.requestId,
    kind: approval.kind,
    details: approval.details,
    engineState: continuationState,
  };
}

export function validateApprovalResolution(
  resolution: EngineApprovalResolution,
  pending: PendingApprovalState,
): string | null {
  if (resolution.approvalRequestId !== pending.requestId) {
    return `Approval request ID mismatch: expected ${pending.requestId}, got ${resolution.approvalRequestId}`;
  }
  return null;
}

/**
 * Extract the typed EngineContinuationState from a resolution's engineState.
 * Handles both the new typed format and legacy untyped snapshots.
 */
export function extractContinuationState(
  resolution: EngineApprovalResolution,
): EngineContinuationState | undefined {
  const raw = resolution.engineState;
  if (!raw) return undefined;

  // Check for new typed format (has version field)
  if (
    typeof raw === "object" &&
    raw !== null &&
    "version" in raw &&
    (raw as EngineContinuationState).version === 1
  ) {
    return raw as EngineContinuationState;
  }

  // Legacy untyped snapshot: try to extract pendingToolSnapshot
  const legacy = raw as Record<string, unknown>;
  const pendingToolSnapshot = legacy?.pendingToolSnapshot as
    | { remainingToolCalls?: unknown[] }
    | undefined;

  if (pendingToolSnapshot?.remainingToolCalls) {
    const calls = pendingToolSnapshot.remainingToolCalls as Array<Record<string, unknown>>;
    return {
      version: 1,
      pendingToolCalls: {
        assistantMessage: { role: "assistant", content: [] } as never,
        remainingToolCallIds: calls.map((tc) => tc.id as string),
        toolCalls: calls.map((tc) => ({
          id: tc.id as string,
          name: tc.name as string,
          args: (tc.arguments as Record<string, unknown>) ?? {},
        })),
        settings: {
          parallelTools: (legacy.pendingToolSettings as { parallelTools?: boolean })?.parallelTools,
        },
      },
    };
  }

  return undefined;
}
