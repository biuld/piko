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
 */
export function extractContinuationState(
  resolution: EngineApprovalResolution,
): EngineContinuationState | undefined {
  const raw = resolution.engineState;
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
