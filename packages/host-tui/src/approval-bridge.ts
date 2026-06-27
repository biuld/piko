import { debugTrace, type ToolApprovalDecision, type ToolApprovalRequest } from "./shared/index.js";

export interface PendingApproval {
  resolve: (decision: ToolApprovalDecision) => void;
  request: ToolApprovalRequest;
  signal?: AbortSignal;
}

/** Lossless bridge between the Host approval gateway and the mounted TUI. */
export function createApprovalBridge() {
  let onPending: ((pending: PendingApproval) => void) | null = null;
  const buffered: PendingApproval[] = [];

  const deliver = (pending: PendingApproval): void => {
    if (!onPending) {
      buffered.push(pending);
      debugTrace({
        stage: "approval.bridge.buffered",
        taskId: pending.request.taskId,
        agentId: pending.request.agentId,
        toolCallId: pending.request.callId,
        toolName: pending.request.toolName,
      });
      return;
    }
    try {
      onPending(pending);
    } catch {
      debugTrace({
        stage: "approval.bridge.delivery_error",
        level: "error",
        taskId: pending.request.taskId,
        agentId: pending.request.agentId,
        toolCallId: pending.request.callId,
        toolName: pending.request.toolName,
      });
      pending.resolve("decline");
    }
  };

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

      deliver(pending);
    });
  };

  return {
    handler,
    onPending(listener: (pending: PendingApproval) => void): void {
      onPending = listener;
      for (const pending of buffered.splice(0)) deliver(pending);
    },
  };
}
