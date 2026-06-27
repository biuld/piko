import type { ApprovalStore } from "../../approval-store.js";
import type { ToolApprovalDecision, ToolApprovalRequest } from "../../shared/index.js";
import { debugTrace } from "../../shared/index.js";
import type { TuiEvent } from "../../state/events.js";
import type { HostdActionAdapter } from "./hostd-action-adapter.js";

type Dispatch = (event: TuiEvent) => void;

export class ApprovalActionController {
  approvalStore?: ApprovalStore;
  onOpenApprovalSurface?: () => string;

  private pendingApprovals = new Map<
    string,
    {
      resolve: (decision: ToolApprovalDecision) => void;
      reject: (err: Error) => void;
      request: ToolApprovalRequest;
    }
  >();

  constructor(
    private readonly hostd: HostdActionAdapter,
    private readonly dispatch: Dispatch,
  ) {}

  approvalHandler = (
    request: ToolApprovalRequest,
    signal?: AbortSignal,
  ): Promise<ToolApprovalDecision> => {
    const callId = request.callId;
    const entityId = request.toolEntityId || callId;

    if (signal?.aborted) {
      return Promise.resolve("decline");
    }

    if (this.approvalStore?.isApproved(request.toolName, request.toolArgs)) {
      return Promise.resolve("accept");
    }

    return new Promise<ToolApprovalDecision>((resolve, reject) => {
      this.pendingApprovals.set(entityId, { resolve, reject, request });
      this.dispatchApprovalNeeded(request);
      this.onOpenApprovalSurface?.();

      if (signal) {
        const onAbort = () => {
          this.pendingApprovals.delete(entityId);
          this.dispatch({
            type: "approval_resolved",
            toolEntityId: entityId,
            callId,
            decision: "decline",
          });
          resolve("decline");
        };
        signal.addEventListener("abort", onAbort, { once: true });

        this.pendingApprovals.set(entityId, {
          resolve: (decision) => {
            signal.removeEventListener("abort", onAbort);
            resolve(decision);
          },
          reject: (error) => {
            signal.removeEventListener("abort", onAbort);
            reject(error);
          },
          request,
        });
      }
    });
  };

  resolveApproval(toolEntityId: string, decision: ToolApprovalDecision): void {
    const entry = this.pendingApprovals.get(toolEntityId);
    if (!entry) {
      if (this.hostd.enabled) {
        this.hostd.respondApproval(toolEntityId, decision);
      }
      return;
    }

    this.pendingApprovals.delete(toolEntityId);
    const callId = entry.request.callId;

    if (decision === "accept_session") {
      this.approvalStore?.grant(entry.request.toolName, entry.request.toolArgs, "session");
    } else if (decision === "accept_workspace") {
      this.approvalStore?.grant(entry.request.toolName, entry.request.toolArgs, "workspace");
    } else if (decision === "accept_permanent") {
      this.approvalStore?.grant(entry.request.toolName, entry.request.toolArgs, "permanent");
    }

    this.dispatch({ type: "approval_resolved", toolEntityId, callId, decision });
    debugTrace({
      stage: "approval.tui.resolved",
      taskId: entry.request.taskId,
      agentId: entry.request.agentId,
      toolCallId: callId,
      toolName: entry.request.toolName,
      status: decision,
    });
    entry.resolve(decision);
  }

  setApprovalBridge(bridge: {
    onPending(
      listener: (pending: {
        resolve: (decision: ToolApprovalDecision) => void;
        request: ToolApprovalRequest;
        signal?: AbortSignal;
      }) => void,
    ): void;
  }): void {
    bridge.onPending((pending) => {
      const callId = pending.request.callId;
      const entityId = pending.request.toolEntityId || callId;
      const onAbort = () => {
        this.pendingApprovals.delete(entityId);
        this.dispatch({
          type: "approval_resolved",
          toolEntityId: entityId,
          callId,
          decision: "decline",
        });
        debugTrace({
          stage: "approval.tui.resolved",
          taskId: pending.request.taskId,
          agentId: pending.request.agentId,
          toolCallId: callId,
          toolName: pending.request.toolName,
          outcome: "aborted",
        });
      };
      const resolve = (decision: ToolApprovalDecision) => {
        pending.signal?.removeEventListener("abort", onAbort);
        pending.resolve(decision);
      };
      this.pendingApprovals.set(entityId, {
        resolve,
        reject: () => {},
        request: pending.request,
      });
      pending.signal?.addEventListener("abort", onAbort, { once: true });
      debugTrace({
        stage: "approval.tui.received",
        taskId: pending.request.taskId,
        agentId: pending.request.agentId,
        toolCallId: callId,
        toolName: pending.request.toolName,
      });
      this.dispatchApprovalNeeded(pending.request);
      this.onOpenApprovalSurface?.();
    });
  }

  private dispatchApprovalNeeded(request: ToolApprovalRequest): void {
    this.dispatch({
      type: "approval_needed",
      toolEntityId: request.toolEntityId,
      callId: request.callId,
      toolName: request.toolName,
      toolArgs: request.toolArgs,
    });
  }
}
