// ---- Task management: dispatch, lifecycle ----

import type {
  AgentTask,
  AgentTaskId,
  AgentTaskResult,
  AgentTaskState,
  WakeReason,
} from "piko-orchestrator-protocol";
import type { OrchestratorCtx } from "../context.js";
import { emitToCtx } from "../context.js";
import { v4Id } from "../id.js";
import { schedule } from "../scheduler/index.js";

export async function dispatch(ctx: OrchestratorCtx, task: AgentTask): Promise<AgentTaskId> {
  const agent = ctx.state.agents[task.targetAgentId];
  if (!agent) {
    throw new Error(`Agent not found: ${task.targetAgentId}`);
  }

  const taskId = task.id ?? v4Id("task");
  const taskState: AgentTaskState = {
    id: taskId,
    targetAgentId: task.targetAgentId,
    prompt: task.prompt,
    source: task.source,
    status: "queued",
    priority: task.priority ?? 0,
    parentTaskId: task.parentTaskId,
  };

  ctx.state.tasks = { ...ctx.state.tasks, [taskId]: taskState };
  const agentState = ctx.state.agents[task.targetAgentId];
  ctx.state.agents = {
    ...ctx.state.agents,
    [task.targetAgentId]: { ...agentState, inbox: [...agentState.inbox, taskId] },
  };

  emitToCtx(ctx, { subsystem: "task", type: "enqueued", task: taskState });
  schedule(ctx);
  return taskId;
}

export async function wake(
  ctx: OrchestratorCtx,
  agentId: string,
  reason: WakeReason,
): Promise<void> {
  const agent = ctx.state.agents[agentId];
  if (!agent) return;

  ctx.state.agents = { ...ctx.state.agents, [agentId]: { ...agent, lastWakeReason: reason } };
  schedule(ctx);
}

export function completeTask(
  ctx: OrchestratorCtx,
  taskId: AgentTaskId,
  result: AgentTaskResult,
): void {
  const task = ctx.state.tasks[taskId];
  if (!task) return;

  ctx.state.tasks = { ...ctx.state.tasks, [taskId]: { ...task, status: "completed", result } };

  // Wake parent task via resource.subagent
  if (task.parentTaskId) {
    const parentTask = ctx.state.tasks[task.parentTaskId];
    if (parentTask) {
      emitToCtx(ctx, {
        subsystem: "resource",
        type: "acquired",
        taskId: parentTask.id,
        agentId: parentTask.targetAgentId,
        item: {
          kind: "subagent",
          id: `dep-${task.parentTaskId}`,
          targetAgentId: task.targetAgentId,
          prompt: "",
        },
        result: {
          kind: "subagent",
          id: `dep-${task.parentTaskId}`,
          agentId: task.targetAgentId,
          result,
        },
      });
    }
  }

  emitToCtx(ctx, {
    subsystem: "task",
    type: "completed",
    taskId,
    agentId: task.targetAgentId,
    result,
    totalSteps: Object.keys(ctx.state.tasks).length,
  });

  const agent = ctx.state.agents[task.targetAgentId];
  if (agent?.activeTaskId === taskId) {
    ctx.state.agents = {
      ...ctx.state.agents,
      [task.targetAgentId]: { ...agent, status: "idle", activeTaskId: undefined },
    };
  }
}

export function failTask(ctx: OrchestratorCtx, taskId: AgentTaskId, error: string): void {
  const task = ctx.state.tasks[taskId];
  if (!task) return;

  ctx.state.tasks = { ...ctx.state.tasks, [taskId]: { ...task, status: "failed", error } };
  emitToCtx(ctx, { subsystem: "task", type: "failed", taskId, agentId: task.targetAgentId, error });

  const agent = ctx.state.agents[task.targetAgentId];
  if (agent?.activeTaskId === taskId) {
    ctx.state.agents = {
      ...ctx.state.agents,
      [task.targetAgentId]: { ...agent, status: "failed", activeTaskId: undefined },
    };
  }
}

export function blockTask(ctx: OrchestratorCtx, taskId: AgentTaskId, reason: string): void {
  const task = ctx.state.tasks[taskId];
  if (!task) return;
  ctx.state.tasks = { ...ctx.state.tasks, [taskId]: { ...task, status: "blocked" } };
  const blockedReason =
    reason === "awaiting_resource" ||
    reason === "awaiting_lock" ||
    reason === "awaiting_approval" ||
    reason === "awaiting_subagent"
      ? reason
      : "awaiting_resource";
  emitToCtx(ctx, {
    subsystem: "task",
    type: "blocked",
    taskId,
    agentId: task.targetAgentId,
    reason: blockedReason,
  });
}
