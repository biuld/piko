import type { HostEvent } from "piko-orchestrator-protocol";
import type { OrchestratorEvent, OrchestratorEventEnvelope, StateActorState } from "./types.js";

function eventOrderFields(event: OrchestratorEvent, env: OrchestratorEventEnvelope) {
  const orchEvent = event as Record<string, unknown>;
  return {
    eventSeq: (typeof orchEvent.eventSeq === "number" ? orchEvent.eventSeq : undefined) ?? env.seq,
    turnIndex: (typeof orchEvent.turnIndex === "number" ? orchEvent.turnIndex : undefined) ?? 0,
    messageIndex: typeof orchEvent.messageIndex === "number" ? orchEvent.messageIndex : undefined,
  };
}

function toolOrderFields(event: OrchestratorEvent) {
  const orchEvent = event as Record<string, unknown>;
  return {
    parentMessageId:
      (typeof orchEvent.parentMessageId === "string" ? orchEvent.parentMessageId : undefined) ?? "",
    contentIndex:
      (typeof orchEvent.contentIndex === "number" ? orchEvent.contentIndex : undefined) ?? 0,
    toolCallIndex:
      (typeof orchEvent.toolCallIndex === "number" ? orchEvent.toolCallIndex : undefined) ?? 0,
  };
}

export function eventToHostEvent(
  event: OrchestratorEvent,
  env: OrchestratorEventEnvelope,
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
        ...eventOrderFields(event, env),
        type: "task_started",
        taskId: event.taskId,
        agentId: event.agentId,
      };
    case "task_message_start":
      return {
        ...eventOrderFields(event, env),
        type: "message_start",
        agentId: event.agentId,
        taskId: event.taskId,
        message: event.message,
      };
    case "task_message_update":
      return {
        ...eventOrderFields(event, env),
        type: "message_update",
        agentId: event.agentId,
        taskId: event.taskId,
        message: event.message,
        assistantEvent: event.assistantEvent,
      };
    case "task_message_end":
      return {
        ...eventOrderFields(event, env),
        type: "message_end",
        agentId: event.agentId,
        taskId: event.taskId,
        message: event.message,
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
        ...eventOrderFields(event, env),
        type: "task_completed",
        taskId: event.taskId,
        agentId: event.agentId,
        result: event.result,
      };
    case "task_transcript_committed":
      return {
        ...eventOrderFields(event, env),
        type: "task_transcript_committed",
        taskId: event.taskId,
        agentId: event.agentId,
        messages: event.messages,
        summary: event.summary,
        finalStatus: event.finalStatus,
      };
    case "task_failed":
      return {
        ...eventOrderFields(event, env),
        type: "task_failed",
        taskId: event.taskId,
        agentId: event.agentId,
        error: event.error,
      };
    case "task_cancelled":
      return {
        ...eventOrderFields(event, env),
        type: "task_failed",
        taskId: event.taskId,
        agentId: event.agentId,
        error: event.reason ?? "Cancelled",
      };
    case "plan_updated":
      return {
        ...eventOrderFields(event, env),
        type: "plan_updated",
        agentId: event.agentId,
        taskId: event.taskId,
        plan: Array.isArray(event.plan) ? event.plan : [],
      };
    case "tool_started": {
      const meta = state.callMetas.get(event.callId);
      return {
        ...eventOrderFields(event, env),
        ...toolOrderFields(event),
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
        ...eventOrderFields(event, env),
        ...toolOrderFields(event),
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
        ...eventOrderFields(event, env),
        type: "approval_needed",
        approvalId: (event.approval as { id?: string })?.id ?? "",
        agentId: "",
        taskId: "",
        toolName: "",
        toolArgs: {},
      };
    case "approval_resolved":
      return {
        ...eventOrderFields(event, env),
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
