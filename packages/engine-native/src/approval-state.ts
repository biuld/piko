import type { PendingApprovalState, EngineInput, EngineApprovalResolution } from "piko-engine-protocol";

export interface ApprovalContext {
  requestId: string;
  kind: string;
  details: unknown;
}

export function createPendingApproval(
  approval: ApprovalContext,
  engineState?: unknown,
): PendingApprovalState {
  return {
    requestId: approval.requestId,
    kind: approval.kind,
    details: approval.details,
    engineState,
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
