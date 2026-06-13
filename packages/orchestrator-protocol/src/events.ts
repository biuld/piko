// ---- Host-visible streaming events ----

import type { AgentTaskResult } from "./agents.js";

export type HostEvent =
  | { type: "token"; agentId: string; taskId: string; text: string }
  | {
      type: "thinking";
      agentId: string;
      taskId: string;
      text: string;
    }
  | {
      type: "tool_start";
      agentId: string;
      taskId: string;
      id: string;
      name: string;
      args: Record<string, unknown>;
    }
  | {
      type: "tool_end";
      agentId: string;
      taskId: string;
      id: string;
      name: string;
      result: unknown;
      isError: boolean;
    }
  | {
      type: "approval_needed";
      approvalId: string;
      agentId: string;
      taskId: string;
      toolName: string;
      toolArgs: Record<string, unknown>;
    }
  | {
      type: "approval_resolved";
      approvalId: string;
      agentId: string;
      taskId: string;
      decision: "accept" | "decline";
    }
  | { type: "task_started"; taskId: string; agentId: string }
  | {
      type: "task_completed";
      taskId: string;
      agentId: string;
      result: AgentTaskResult;
    }
  | {
      type: "task_failed";
      taskId: string;
      agentId: string;
      error: string;
    }
  | { type: "done"; status: string };

export type HostEventListener = (event: HostEvent) => void;
