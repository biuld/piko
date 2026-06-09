// ---- Task management: dispatch, lifecycle ----

import type {
  AgentTask,
  AgentTaskId,
  AgentTaskResult,
  AgentTaskState,
  WakeReason,
} from "piko-engine-protocol";
import { v4Id } from "../id.js";
import type { OrchestratorCtx } from "./context.js";
import { emitToCtx } from "./context.js";
import { schedule } from "./scheduler.js";

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

  emitToCtx(ctx, { type: "task_enqueued", task: taskState });
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

  // Wake parent task
  if (task.parentTaskId) {
    const parentTask = ctx.state.tasks[task.parentTaskId];
    if (parentTask) {
      emitToCtx(ctx, {
        type: "watch_triggered",
        watchId: `dep-${task.parentTaskId}`,
        agentId: parentTask.targetAgentId,
        reason: { kind: "subagent_result", fromAgentId: task.targetAgentId, taskId },
      });
    }
  }

  emitToCtx(ctx, { type: "task_completed", taskId, agentId: task.targetAgentId, result });

  const agent = ctx.state.agents[task.targetAgentId];
  if (agent?.activeTaskId === taskId) {
    ctx.state.agents = {
      ...ctx.state.agents,
      [task.targetAgentId]: { ...agent, status: "idle", activeTaskId: undefined },
    };
    emitToCtx(ctx, {
      type: "agent_status_changed",
      agentId: task.targetAgentId,
      from: "running",
      to: "idle",
    });
  }
}

export function failTask(ctx: OrchestratorCtx, taskId: AgentTaskId, error: string): void {
  const task = ctx.state.tasks[taskId];
  if (!task) return;

  ctx.state.tasks = { ...ctx.state.tasks, [taskId]: { ...task, status: "failed", error } };
  emitToCtx(ctx, { type: "task_failed", taskId, agentId: task.targetAgentId, error });

  const agent = ctx.state.agents[task.targetAgentId];
  if (agent?.activeTaskId === taskId) {
    ctx.state.agents = {
      ...ctx.state.agents,
      [task.targetAgentId]: { ...agent, status: "failed", activeTaskId: undefined },
    };
    emitToCtx(ctx, {
      type: "agent_status_changed",
      agentId: task.targetAgentId,
      from: "running",
      to: "failed",
    });
  }
}

export function blockTask(ctx: OrchestratorCtx, taskId: AgentTaskId, reason: string): void {
  const task = ctx.state.tasks[taskId];
  if (!task) return;
  ctx.state.tasks = { ...ctx.state.tasks, [taskId]: { ...task, status: "blocked" } };
  emitToCtx(ctx, { type: "task_blocked", taskId, agentId: task.targetAgentId, reason });
}
