import {
  debugTrace,
  type ToolApprovalDecision,
  type ToolApprovalRequest,
} from "piko-orchestrator-protocol";

export interface PendingApproval {
  resolve: (decision: ToolApprovalDecision) => void;
  request: ToolApprovalRequest;
  signal?: AbortSignal;
}

/** Lossless bridge between the Host approval gateway and the mounted TUI. */
export function createApprovalBridge() {
  let onPending: ((pending: PendingApproval) => void) | null = null;
  const buffered: PendingApproval[] = [];

  const handler = (
    request: ToolApprovalRequest,
    signal?: AbortSignal,
  ): Promise<ToolApprovalDecision> => {
    if (signal?.aborted) return Promise.resolve("decline");

    debugTrace({
      stage: "approval.bridge.requested",
      taskId: request.taskId,
      agentId: request.agentId,
      toolCallId: request.callId,
      toolName: request.toolName,
    });

    return new Promise<ToolApprovalDecision>((resolve) => {
      let settled = false;
      let pending: PendingApproval;
      const settle = (decision: ToolApprovalDecision) => {
        if (settled) return;
        settled = true;
        const bufferedIndex = buffered.indexOf(pending);
        if (bufferedIndex >= 0) buffered.splice(bufferedIndex, 1);
        debugTrace({
          stage: "approval.bridge.resolved",
          taskId: request.taskId,
          agentId: request.agentId,
          toolCallId: request.callId,
          toolName: request.toolName,
          status: decision,
        });
        resolve(decision);
      };
      pending = { resolve: settle, request, signal };

      signal?.addEventListener("abort", () => settle("decline"), { once: true });

      if (onPending) {
        onPending(pending);
      } else {
        buffered.push(pending);
        debugTrace({
          stage: "approval.bridge.buffered",
          taskId: request.taskId,
          agentId: request.agentId,
          toolCallId: request.callId,
          toolName: request.toolName,
        });
      }
    });
  };

  return {
    handler,
    onPending(listener: (pending: PendingApproval) => void): void {
      onPending = listener;
      for (const pending of buffered.splice(0)) listener(pending);
    },
  };
}
