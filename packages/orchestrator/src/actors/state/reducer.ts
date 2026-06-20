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

function updateAgentActiveStatus(state: StateActorState, agentId: string) {
  const agent = state.agents[agentId];
  if (!agent) return;

  const runningTasks = Object.values(state.tasks).filter(
    (t) => t.targetAgentId === agentId && t.status === "running",
  );

  if (runningTasks.length > 0) {
    agent.status = "running";
    agent.activeTaskId = runningTasks[runningTasks.length - 1].id;
  } else {
    agent.status = "idle";
    agent.activeTaskId = undefined;
  }
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
      updateAgentActiveStatus(state, event.agentId);
      break;
    }
    case "task_completed": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.status = "completed";
        task.result = event.result;
      }
      updateAgentActiveStatus(state, event.agentId);
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
      updateAgentActiveStatus(state, event.agentId);
      break;
    }
    case "task_cancelled": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.status = "cancelled";
        task.error = event.reason ?? "Cancelled";
      }
      updateAgentActiveStatus(state, event.agentId);
      break;
    }
    case "plan_updated": {
      const task = state.tasks[event.taskId];
      if (task) {
        task.plan = Array.isArray(event.plan) ? event.plan : [];
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
