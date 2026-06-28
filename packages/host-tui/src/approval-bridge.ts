import type { ToolApprovalDecision, ToolApprovalRequest } from "./shared/index.js";

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
      return;
    }
    try {
      onPending(pending);
    } catch {
      pending.resolve("decline");
    }
  };

  const handler = (
    request: ToolApprovalRequest,
    signal?: AbortSignal,
  ): Promise<ToolApprovalDecision> => {
    if (signal?.aborted) return Promise.resolve("decline");

    return new Promise<ToolApprovalDecision>((resolve) => {
      let settled = false;
      let pending: PendingApproval;
      const settle = (decision: ToolApprovalDecision) => {
        if (settled) return;
        settled = true;
        const bufferedIndex = buffered.indexOf(pending);
        if (bufferedIndex >= 0) buffered.splice(bufferedIndex, 1);
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
