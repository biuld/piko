import type { HostEvent } from "piko-orchestrator-protocol";
import type { OrchestratorEvent, OrchestratorEventEnvelope, StateActorState } from "./types.js";

export function eventToHostEvent(
  event: OrchestratorEvent,
  _env: OrchestratorEventEnvelope,
  state: StateActorState,
): HostEvent | null {
  switch (event.type) {
    case "orchestrator_started":
      return null;
    case "orchestrator_stopped":
      return { type: "done", status: "stopped" };
    case "agent_registered":
      return null;
    case "agent_unregistered":
      return null;
    case "tool_set_registered":
      return null;
    case "tool_set_unregistered":
      return null;
    case "task_created":
      return {
        type: "task_created",
        task: {
          ...event.task,
          id: event.task.id ?? "",
          targetAgentId: event.task.targetAgentId,
        },
      };
    case "task_started":
      return {
        type: "task_started",
        taskId: event.taskId,
        agentId: event.agentId,
      };
    case "task_delta": {
      const delta = event.delta as { kind?: string; text?: string };
      if (delta?.kind === "thinking" && delta.text) {
        return {
          type: "thinking",
          agentId: event.agentId,
          taskId: event.taskId,
          text: delta.text,
        };
      }
      if (delta?.text) {
        return {
          type: "token",
          agentId: event.agentId,
          taskId: event.taskId,
          text: delta.text,
        };
      }
      return null;
    }
    case "task_completed":
      return {
        type: "task_completed",
        taskId: event.taskId,
        agentId: event.agentId,
        result: event.result,
      };
    case "task_transcript_committed":
      return {
        type: "task_transcript_committed",
        taskId: event.taskId,
        agentId: event.agentId,
        messages: event.messages,
        summary: event.summary,
        finalStatus: event.finalStatus,
      };
    case "task_failed":
      return {
        type: "task_failed",
        taskId: event.taskId,
        agentId: event.agentId,
        error: event.error,
      };
    case "task_cancelled":
      return {
        type: "task_failed",
        taskId: event.taskId,
        agentId: event.agentId,
        error: event.reason ?? "Cancelled",
      };
    case "tool_started": {
      const meta = state.callMetas.get(event.callId);
      return {
        type: "tool_start",
        agentId: event.agentId,
        taskId: event.taskId,
        id: event.callId,
        name: event.name,
        args: meta?.args ?? {},
      };
    }
    case "tool_finished": {
      const meta = state.callMetas.get(event.callId);
      const result = event.result as { ok?: boolean; error?: unknown } | undefined;
      return {
        type: "tool_end",
        agentId: event.agentId,
        taskId: event.taskId,
        id: event.callId,
        name: meta?.name ?? "",
        result,
        isError: result && typeof result === "object" && "ok" in result ? !result.ok : false,
      };
    }
    case "approval_requested":
      return {
        type: "approval_needed",
        approvalId: (event.approval as { id?: string })?.id ?? "",
        agentId: "",
        taskId: "",
        toolName: "",
        toolArgs: {},
      };
    case "approval_resolved":
      return {
        type: "approval_resolved",
        approvalId: event.approvalId,
        agentId: "",
        taskId: "",
        decision: event.decision as "accept" | "decline",
      };
    default:
      return null;
  }
}
