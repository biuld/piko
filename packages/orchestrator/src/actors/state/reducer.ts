import type { AgentTaskResult } from "piko-orchestrator-protocol";
import type { OrchestratorEventEnvelope, StateActorState } from "./types.js";

export function createInitialState(runId: string): StateActorState {
  return {
    runId,
    status: "idle",
    eventLog: [],
    seq: 0,
    agents: {},
    tasks: {},
    locks: {},
    listeners: new Map(),
    nextSubId: 1,
    callMetas: new Map(),
    toolSets: {},
  };
}

export function reduceStateEvent(state: StateActorState, env: OrchestratorEventEnvelope): void {
  const event = env.event;

  switch (event.type) {
    case "orchestrator_started":
      state.status = "running";
      break;
    case "orchestrator_stopped":
      state.status = "stopped";
      break;
    case "agent_registered": {
      const existing = state.agents[event.agent.id];
      state.agents[event.agent.id] = {
        id: event.agent.id,
        spec: event.agent,
        status: "idle",
        transcript: existing?.transcript ?? [],
      };
      break;
    }
    case "agent_unregistered":
      delete state.agents[event.agentId];
      break;
    case "tool_set_registered":
      state.toolSets[event.toolSet.id] = event.toolSet;
      break;
    case "tool_set_unregistered":
      delete state.toolSets[event.toolSetId];
      break;
    case "task_created": {
      const task = event.task;
      const taskId = task.id ?? `task_${env.seq}`;
      state.tasks[taskId] = {
        id: taskId,
        targetAgentId: task.targetAgentId,
        prompt: task.prompt,
        source: task.source,
        status: "queued",
        priority: task.priority ?? 0,
        parentTaskId: task.parentTaskId,
      };
      break;
    }
    case "task_started": {
      const task = state.tasks[event.taskId];
      if (task) task.status = "running";
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.status = "running";
        agent.activeTaskId = event.taskId;
      }
      break;
    }
    case "task_completed": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.status = "completed";
        task.result = event.result;
      }
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.status = "idle";
        agent.activeTaskId = undefined;
      }
      break;
    }
    case "task_transcript_committed": {
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.transcript = event.messages;
      }
      break;
    }
    case "task_failed": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.status = "failed";
        task.error = event.error;
      }
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.status = "idle";
        agent.activeTaskId = undefined;
      }
      break;
    }
    case "task_cancelled": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.status = "cancelled";
        task.error = event.reason ?? "Cancelled";
      }
      const agent = state.agents[event.agentId];
      if (agent) {
        agent.status = "idle";
        agent.activeTaskId = undefined;
      }
      break;
    }
    case "plan_updated": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.result = {
          ...task.result,
          summary: task.result?.summary ?? "",
          artifacts: task.result?.artifacts ?? [],
          plan: event.plan,
        } as AgentTaskResult & { plan: unknown };
      }
      break;
    }
    case "tool_started": {
      state.callMetas.set(event.callId, {
        name: event.name,
        args: event.args ?? {},
      });
      break;
    }
    case "tool_finished":
    case "approval_requested":
    case "approval_resolved":
    case "actor_spawned":
    case "actor_stopped":
    case "actor_error":
      break;
  }
}
