// ---- Approval gateway protocol types ----
// Host-visible approval types.

export interface ToolApprovalRequest {
  toolEntityId: string;
  callId: string;
  agentId: string;
  taskId: string;
  toolName: string;
  toolArgs: Record<string, unknown>;
}

export type ToolApprovalDecision =
  | "accept"
  | "decline"
  | "accept_session"
  | "accept_workspace"
  | "accept_permanent";

export function isApprovalAccepted(decision: ToolApprovalDecision): boolean {
  return decision !== "decline";
}

export interface ApprovalGateway {
  requestToolApproval(
    request: ToolApprovalRequest,
    signal?: AbortSignal,
  ): Promise<ToolApprovalDecision>;
}
