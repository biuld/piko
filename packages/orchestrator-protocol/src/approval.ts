// ---- Approval gateway protocol types ----
// Host-visible approval types.

export interface ToolApprovalRequest {
  callId: string;
  agentId: string;
  taskId: string;
  toolName: string;
  toolArgs: Record<string, unknown>;
}

export type ToolApprovalDecision = "accept" | "decline";

export interface ApprovalGateway {
  requestToolApproval(request: ToolApprovalRequest): Promise<ToolApprovalDecision>;
}
