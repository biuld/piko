import type {
  AgentTask,
  AgentTaskId,
  Message,
  OrchRunOptions,
  OrchRunResult,
} from "piko-orchestrator-protocol";
import type { OrchestratorContext } from "./context.js";

export async function dispatch(ctx: OrchestratorContext, task: AgentTask): Promise<AgentTaskId> {
  const { taskId, targetAgentId, normalizedTask } = normalizeTask(ctx, task);
  await emitTaskCreated(ctx, normalizedTask);
  await askAgent(ctx, targetAgentId, normalizedTask);

  return taskId;
}

export async function dispatchDetached(
  ctx: OrchestratorContext,
  task: AgentTask,
): Promise<AgentTaskId> {
  const { taskId, targetAgentId, normalizedTask } = normalizeTask(ctx, task);
  await emitTaskCreated(ctx, normalizedTask);
  trackDetached(ctx, taskId, dispatchLater(ctx, targetAgentId, normalizedTask));

  return taskId;
}

export async function delegateToAgent(
  ctx: OrchestratorContext,
  task: AgentTask,
): Promise<{ taskId: string; result: unknown }> {
  const { taskId, targetAgentId, normalizedTask } = normalizeTask(ctx, task);
  await emitTaskCreated(ctx, normalizedTask);
  const result = await askAgent(ctx, targetAgentId, normalizedTask);

  return { taskId, result };
}

export async function delegateDetached(ctx: OrchestratorContext, task: AgentTask): Promise<string> {
  const { taskId, targetAgentId, normalizedTask } = normalizeTask(ctx, task);
  await emitTaskCreated(ctx, normalizedTask);
  trackDetached(ctx, taskId, dispatchLater(ctx, targetAgentId, normalizedTask));

  return taskId;
}

export async function joinTask(ctx: OrchestratorContext, taskId: string): Promise<unknown> {
  const handle = ctx.detachedTasks.get(taskId);
  if (!handle) {
    throw new Error(`Detached task not found: ${taskId}`);
  }
  if (handle.resolved) {
    ctx.detachedTasks.delete(taskId);
    return handle.result;
  }
  const result = await handle.promise;
  ctx.detachedTasks.delete(taskId);
  return result;
}

export async function run(
  ctx: OrchestratorContext,
  prompt: string,
  opts?: OrchRunOptions,
): Promise<OrchRunResult> {
  const targetAgentId = opts?.targetAgentId || ctx.defaultAgentId;
  const signal = opts?.signal;

  if (!ctx.system.hasActor(`agent:${targetAgentId}`)) {
    throw new Error(`Agent "${targetAgentId}" not registered.`);
  }

  await ctx.emit({ type: "orchestrator_started" });

  const taskId = `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
  const task: AgentTask = {
    id: taskId,
    targetAgentId,
    prompt,
    source: { type: "user" },
    history: opts?.history,
  };

  await ctx.emit({ type: "task_created", task });

  const onAbort = () => {
    try {
      ctx.system.send(`agent:${targetAgentId}`, {
        type: "cancel",
        taskId,
        reason: "Aborted by signal",
      });
    } catch {}
  };

  if (signal) {
    if (signal.aborted) {
      return buildRunResult([], 0, "aborted");
    }
    signal.addEventListener("abort", onAbort, { once: true });
  }

  try {
    const agentResult = await ctx.system.ask<{
      messages: Message[];
      totalSteps: number;
      finalStatus: string;
    }>(`agent:${targetAgentId}`, { type: "dispatch", task });

    return buildRunResult(
      agentResult.messages ?? [],
      agentResult.totalSteps ?? 1,
      mapStatus(agentResult.finalStatus),
    );
  } catch (err) {
    const errorMsg = err instanceof Error ? err.message : String(err);
    await ctx.emit({
      type: "task_failed",
      agentId: targetAgentId,
      taskId,
      error: errorMsg,
    });
    return buildRunResult([], 0, "error");
  } finally {
    if (signal) {
      signal.removeEventListener("abort", onAbort);
    }
  }
}

async function cancelTask(
  ctx: OrchestratorContext,
  taskId: string,
  reason?: string,
): Promise<void> {
  const taskState = ctx.stateCache.tasks[taskId];
  if (!taskState) {
    throw new Error(`Task "${taskId}" not found`);
  }
  await ctx.system.ask(`agent:${taskState.targetAgentId}`, {
    type: "cancel",
    taskId,
    reason,
  });
}

// Re-export this so it's accessible or used internally
export { cancelTask };

function buildRunResult(
  messages: Message[],
  totalSteps: number,
  status: "completed" | "aborted" | "error" | "max_steps",
): OrchRunResult {
  return { messages, totalSteps, status };
}

function normalizeTask(
  ctx: OrchestratorContext,
  task: AgentTask,
): { taskId: string; targetAgentId: string; normalizedTask: AgentTask } {
  const taskId = task.id ?? `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
  const targetAgentId = task.targetAgentId || ctx.defaultAgentId;
  return {
    taskId,
    targetAgentId,
    normalizedTask: {
      ...task,
      id: taskId,
      targetAgentId,
    },
  };
}

async function emitTaskCreated(ctx: OrchestratorContext, task: AgentTask): Promise<void> {
  await ctx.emit({ type: "task_created", task });
}

function askAgent(
  ctx: OrchestratorContext,
  targetAgentId: string,
  task: AgentTask,
): Promise<unknown> {
  return ctx.system.ask<unknown>(`agent:${targetAgentId}`, {
    type: "dispatch",
    task,
  });
}

function dispatchLater(
  ctx: OrchestratorContext,
  targetAgentId: string,
  task: AgentTask,
): Promise<unknown> {
  // Delay lookup to allow ActorNotFoundError to be handled asynchronously.
  return Promise.resolve().then(() => askAgent(ctx, targetAgentId, task));
}

function trackDetached(
  ctx: OrchestratorContext,
  taskId: string,
  resultPromise: Promise<unknown>,
): void {
  const handle = { promise: resultPromise, resolved: false, result: undefined as unknown };
  resultPromise
    .then((result) => {
      handle.result = result;
      handle.resolved = true;
    })
    .catch((err) => {
      handle.result = { error: String(err) };
      handle.resolved = true;
    });
  ctx.detachedTasks.set(taskId, handle);
}

function mapStatus(s: string): "completed" | "aborted" | "error" | "max_steps" {
  switch (s) {
    case "completed":
      return "completed";
    case "aborted":
      return "aborted";
    case "error":
      return "error";
    case "max_steps":
      return "max_steps";
    default:
      return "completed";
  }
}
