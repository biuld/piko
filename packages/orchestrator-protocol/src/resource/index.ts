// ---- resource subsystem ----

import type { AgentTaskResult, LockMode } from "../agents.js";
import type { ApprovalRuntimeState, OrchestratorState } from "../state.js";

// ---- ResourceItem — unified resource descriptor ----

export type ResourceItem =
  | { kind: "tool"; id: string; name: string; args: Record<string, unknown> }
  | { kind: "lock"; id: string; resource: string; mode: LockMode }
  | { kind: "approval"; id: string; toolCallId?: string; toolName?: string; details: unknown }
  | { kind: "subagent"; id: string; targetAgentId: string; prompt: string };

// ---- ResourceResult — unified resource result ----

export type ResourceResult =
  | { kind: "tool"; id: string; name: string; result: unknown; isError: boolean }
  | { kind: "lock"; id: string; resource: string; granted: boolean }
  | { kind: "approval"; id: string; decision: "accept" | "decline" | "acceptForSession" }
  | { kind: "subagent"; id: string; agentId: string; result: AgentTaskResult };

// ---- OrchResourceEvent ----

export type OrchResourceEvent =
  | {
      subsystem: "resource";
      type: "requested";
      taskId: string;
      agentId: string;
      items: ResourceItem[];
    }
  | {
      subsystem: "resource";
      type: "acquired";
      taskId: string;
      agentId: string;
      item: ResourceItem;
      result: ResourceResult;
    }
  | {
      subsystem: "resource";
      type: "declined";
      taskId: string;
      agentId: string;
      item: ResourceItem;
      reason: string;
    }
  | {
      subsystem: "resource";
      type: "resolved";
      taskId: string;
      agentId: string;
    };

// ---- Reducer ----

export function reduceResource(
  state: OrchestratorState,
  event: OrchResourceEvent,
): OrchestratorState {
  switch (event.type) {
    case "requested": {
      const approvals: Record<string, ApprovalRuntimeState> = { ...state.approvals };
      for (const item of event.items) {
        if (item.kind === "approval") {
          approvals[item.id] = {
            id: item.id,
            agentId: event.agentId,
            taskId: event.taskId,
            details: item.details,
            status: "pending",
          };
        }
      }
      return { ...state, approvals };
    }

    case "acquired": {
      if (event.item.kind === "lock" && event.result.kind === "lock" && event.result.granted) {
        const lock = state.locks[event.item.id];
        if (lock) {
          return {
            ...state,
            locks: {
              ...state.locks,
              [event.item.id]: {
                ...lock,
                holderAgentId: event.agentId,
                holderTaskId: event.taskId,
              },
            },
          };
        }
      }
      if (event.item.kind === "approval") {
        const approval = state.approvals[event.item.id];
        if (approval) {
          return {
            ...state,
            approvals: {
              ...state.approvals,
              [event.item.id]: { ...approval, status: "resolved" },
            },
          };
        }
      }
      return state;
    }

    case "declined": {
      if (event.item.kind === "approval") {
        const approval = state.approvals[event.item.id];
        if (approval) {
          return {
            ...state,
            approvals: {
              ...state.approvals,
              [event.item.id]: { ...approval, status: "resolved" },
            },
          };
        }
      }
      return state;
    }

    case "resolved":
      return state;
  }
}
