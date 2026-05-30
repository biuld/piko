import type {
  PendingApprovalState,
  EngineApprovalResolution,
  Message,
} from "piko-engine-protocol";

export type ApprovalDecision = "accept" | "decline" | "acceptForSession";

export interface ApprovalHandler {
  requestApproval(state: PendingApprovalState): Promise<ApprovalDecision>;
}

export function createApprovalResolution(
  runId: string,
  stepId: string,
  pending: PendingApprovalState,
  decision: ApprovalDecision,
  transcript: Message[],
): EngineApprovalResolution {
  return {
    runId,
    stepId,
    approvalRequestId: pending.requestId,
    decision,
    transcript,
    engineState: pending.engineState,
  };
}

/**
 * Auto-accept approval handler for non-interactive mode.
 */
export function createAutoAcceptHandler(): ApprovalHandler {
  return {
    async requestApproval(_state: PendingApprovalState): Promise<ApprovalDecision> {
      return "accept";
    },
  };
}

/**
 * Approval handler that always declines.
 */
export function createAutoDeclineHandler(): ApprovalHandler {
  return {
    async requestApproval(_state: PendingApprovalState): Promise<ApprovalDecision> {
      return "decline";
    },
  };
}
