// ---- Orchestrator task execution — dispatch/run/cancel ----

import type {
  AgentTask,
  AgentTaskId,
  Message,
  OrchRunOptions,
  OrchRunResult,
} from "piko-orchestrator-protocol";
import { agentActor } from "../actors/agent/index.js";
import type { ActorHandler } from "../kernel/actor-system.js";
import type { OrchestratorContext, RunHandle } from "./context.js";

export class TaskAdmissionError extends Error {
  constructor(
    readonly code: "agent_busy" | "concurrency_limit",
    message: string,
  ) {
    super(message);
    this.name = "TaskAdmissionError";
  }
}

export function createRun(
  ctx: OrchestratorContext,
  task: AgentTask,
  options?: { retainForJoin?: boolean },
): RunHandle {
  const { taskId, targetAgentId, normalizedTask } = normalizeTask(ctx, task);
  const spec = ctx.agentSpecs.get(targetAgentId);

  if (ctx.allocatedTaskIds.has(taskId)) {
    throw new Error(`Duplicate task ID: ${taskId}`);
  }

  assertTaskCanStart(ctx, targetAgentId);
  ctx.allocatedTaskIds.add(taskId);

  // Cleanup old settled runs to prevent memory leak
  if (ctx.runs.size >= 100) {
    for (const [id, r] of ctx.runs.entries()) {
      if (
        !r.retainForJoin &&
        r.status !== "running" &&
        r.status !== "cancelling" &&
        r.status !== "starting"
      ) {
        ctx.runs.delete(id);
        if (ctx.runs.size < 100) break;
      }
    }
  }

  void ctx.emit({ type: "task_created", task: normalizedTask });

  const actorId = `agent:${targetAgentId}:task:${taskId}`;

  const runHandle: RunHandle = {
    taskId,
    agentId: targetAgentId,
    actorId,
    status: "starting",
    retainForJoin: options?.retainForJoin ?? false,
    resultPromise: Promise.resolve(), // Will be overwritten below
  };

  const resultPromise = (async () => {
    if (!spec) {
      runHandle.status = "failed";
      await ctx.emit({
        type: "task_failed",
        agentId: targetAgentId,
        taskId,
        turnIndex: 0,
        error: `Agent "${targetAgentId}" not registered.`,
      });
      throw new Error(`Agent "${targetAgentId}" not registered.`);
    }

    const handler = agentActor(spec, ctx.createAgentDeps());

    ctx.system.spawn({
      id: actorId,
      kind: "agent",
      handler: handler as ActorHandler,
    });

    runHandle.status = "running";

    try {
      const res = await ctx.system.ask<any>(actorId, {
        type: "dispatch",
        task: normalizedTask,
      });
      runHandle.status =
        res.finalStatus === "aborted"
          ? "cancelled"
          : res.finalStatus === "completed"
            ? "completed"
            : "failed";
      return res;
    } catch (err) {
      runHandle.status = "failed";
      const errorMsg = err instanceof Error ? err.message : String(err);
      await ctx.emit({
        type: "task_failed",
        agentId: targetAgentId,
        taskId,
        turnIndex: 0,
        error: errorMsg,
      });
      throw err;
    }
  })();

  // Prevent unhandled promise rejection warnings
  resultPromise.catch(() => {});

  runHandle.resultPromise = resultPromise;

  ctx.runs.set(taskId, runHandle);

  return runHandle;
}

function assertTaskCanStart(ctx: OrchestratorContext, targetAgentId: string): void {
  const activeAgentIds = new Set<string>();
  for (const run of ctx.runs.values()) {
    if (run.status === "starting" || run.status === "running" || run.status === "cancelling") {
      activeAgentIds.add(run.agentId);
    }
  }

  if (activeAgentIds.has(targetAgentId)) {
    throw new TaskAdmissionError(
      "agent_busy",
      `Agent "${targetAgentId}" is currently running a task.`,
    );
  }

  if (activeAgentIds.size >= ctx.maxConcurrentAgents) {
    throw new TaskAdmissionError(
      "concurrency_limit",
      `Orchestrator concurrency limit reached (${ctx.maxConcurrentAgents} active agents).`,
    );
  }
}

export async function dispatch(ctx: OrchestratorContext, task: AgentTask): Promise<AgentTaskId> {
  const run = createRun(ctx, task);
  return run.taskId;
}

export async function dispatchDetached(
  ctx: OrchestratorContext,
  task: AgentTask,
): Promise<AgentTaskId> {
  const run = createRun(ctx, task, { retainForJoin: true });
  return run.taskId;
}

export async function delegateToAgent(
  ctx: OrchestratorContext,
  task: AgentTask,
): Promise<{ taskId: string; result: unknown }> {
  const run = createRun(ctx, task);
  const result = await run.resultPromise;
  return { taskId: run.taskId, result };
}

export async function delegateDetached(ctx: OrchestratorContext, task: AgentTask): Promise<string> {
  const run = createRun(ctx, task, { retainForJoin: true });
  return run.taskId;
}

export async function joinTask(ctx: OrchestratorContext, taskId: string): Promise<unknown> {
  const run = ctx.runs.get(taskId);
  if (!run) {
    throw new Error(`Detached task not found: ${taskId}`);
  }
  const res = await run.resultPromise;
  if (res.finalStatus === "error" || res.finalStatus === "failed") {
    throw new Error(res.summary || "Task failed");
  }
  return res;
}

export async function run(
  ctx: OrchestratorContext,
  prompt: string,
  opts?: OrchRunOptions,
): Promise<OrchRunResult> {
  const targetAgentId = opts?.targetAgentId || ctx.defaultAgentId;
  const signal = opts?.signal;

  await ctx.emit({ type: "orchestrator_started" });

  if (signal?.aborted) {
    return buildRunResult([], 0, "aborted");
  }

  const taskId = `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
  const task: AgentTask = {
    id: taskId,
    targetAgentId,
    prompt,
    source: { type: "user" },
    history: opts?.history,
  };

  const runHandle = createRun(ctx, task);

  const onAbort = () => {
    try {
      ctx.system.send(runHandle.actorId, {
        type: "cancel",
        taskId,
        reason: "Aborted by signal",
      });
    } catch {}
  };

  if (signal) {
    if (signal.aborted) {
      onAbort();
    } else {
      signal.addEventListener("abort", onAbort, { once: true });
    }
  }

  try {
    const agentResult = await runHandle.resultPromise;
    return buildRunResult(
      agentResult.messages ?? [],
      agentResult.totalSteps ?? 1,
      mapStatus(agentResult.finalStatus),
    );
  } catch (_err) {
    return buildRunResult([], 0, "error");
  } finally {
    if (signal) {
      signal.removeEventListener("abort", onAbort);
    }
  }
}

export async function cancelTask(
  ctx: OrchestratorContext,
  taskId: string,
  reason?: string,
): Promise<void> {
  const run = ctx.runs.get(taskId);
  if (!run) {
    throw new Error(`Task "${taskId}" not found`);
  }
  if (run.status === "completed" || run.status === "failed" || run.status === "cancelled") {
    return;
  }
  const oldStatus = run.status;
  run.status = "cancelling";
  try {
    await ctx.system.ask(run.actorId, {
      type: "cancel",
      taskId,
      reason,
    });
  } catch (err) {
    run.status = oldStatus;
    throw err;
  }
}

function buildRunResult(
  messages: Message[],
  totalSteps: number,
  status: "completed" | "aborted" | "error",
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

function mapStatus(s: string): "completed" | "aborted" | "error" {
  switch (s) {
    case "completed":
      return "completed";
    case "aborted":
      return "aborted";
    case "error":
      return "error";
    default:
      return "completed";
  }
}
