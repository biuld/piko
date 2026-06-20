// ---- Host-visible streaming events ----

import type { AgentTask, AgentTaskResult } from "./agents.js";
import type { Message } from "./messages.js";
import type {
  RuntimeAssistantMessageEvent,
  RuntimeMessage,
  RuntimeToolOrder,
} from "./runtime-stream.js";

// ---- Shared ordering base for all HostEvent variants -------
interface HostOrderBase {
  /** Strictly increasing within runId (matches RuntimeOrder.eventSeq). */
  eventSeq?: number;

  /** Zero-based turn index. */
  turnIndex?: number;

  /** Stable message position within the run (assigned at message_start). */
  messageIndex?: number;
}

export type HostEvent =
  | (HostOrderBase & {
      type: "message_start";
      agentId: string;
      taskId: string;
      message: RuntimeMessage;
    })
  | (HostOrderBase & {
      type: "message_update";
      agentId: string;
      taskId: string;
      message: RuntimeMessage;
      assistantEvent?: RuntimeAssistantMessageEvent;
    })
  | (HostOrderBase & {
      type: "message_end";
      agentId: string;
      taskId: string;
      message: RuntimeMessage;
    })
  | { type: "token"; agentId: string; taskId: string; text: string }
  | {
      type: "thinking";
      agentId: string;
      taskId: string;
      text: string;
    }
  | (HostOrderBase &
      RuntimeToolOrder & {
        type: "tool_start";
        agentId: string;
        taskId: string;
        id: string;
        name: string;
        args: Record<string, unknown>;
      })
  | (HostOrderBase &
      RuntimeToolOrder & {
        type: "tool_end";
        agentId: string;
        taskId: string;
        id: string;
        name: string;
        result: unknown;
        isError: boolean;
      })
  | (HostOrderBase & {
      type: "approval_needed";
      approvalId: string;
      agentId: string;
      taskId: string;
      toolName: string;
      toolArgs: Record<string, unknown>;
    })
  | (HostOrderBase & {
      type: "approval_resolved";
      approvalId: string;
      agentId: string;
      taskId: string;
      decision: "accept" | "decline";
    })
  | (HostOrderBase & { type: "task_started"; taskId: string; agentId: string })
  | {
      type: "task_created";
      task: AgentTask & { id: string; targetAgentId: string };
    }
  | (HostOrderBase & {
      type: "task_completed";
      taskId: string;
      agentId: string;
      result: AgentTaskResult;
    })
  | (HostOrderBase & {
      type: "task_transcript_committed";
      taskId: string;
      agentId: string;
      messages: Message[];
      summary: string;
      finalStatus: string;
    })
  | (HostOrderBase & {
      type: "task_failed";
      taskId: string;
      agentId: string;
      error: string;
    })
  | { type: "done"; status: string };

export type HostEventListener = (event: HostEvent) => void;
