// ---- Engine step executor: runs engine for all running agents ----

import type { AgentRuntimeState, EngineInput, EngineStepResult } from "piko-engine-protocol";
import type { OrchestratorCtx } from "./context.js";
import { emitToCtx } from "./context.js";
import { completeTask, failTask } from "./tasks.js";

interface PendingApproval {
  approvalId: string;
  taskId: string;
  details: unknown;
  engineState: unknown;
}

/**
 * Run one engine step for an agent and handle the result
 * (append transcript, complete/fail/approve based on status).
 *
 * @returns the result for caller-side tracking
 */
export async function runAndHandleStep(
  ctx: OrchestratorCtx,
  agentId: string,
  taskId: string,
  signal?: AbortSignal,
): Promise<EngineStepResult> {
  const agent = ctx.state.agents[agentId];
  const config = ctx.engineConfig;
  const stepId = `step-${taskId}-${Date.now()}`;
  const input: EngineInput = {
    runId: ctx.state.runId,
    stepId,
    transcript: agent.transcript,
    systemPrompt: agent.spec.systemPrompt,
    model: config?.model ?? ({} as never),
    provider: config?.provider ?? {},
    toolSets: agent.spec.toolSetIds.map((id) => ctx.state.toolSets[id]).filter(Boolean),
    settings: config?.settings ?? { maxSteps: 10, allowToolCalls: true, allowApprovals: true },
  };

  emitToCtx(ctx, { type: "engine_step_started", agentId, taskId, stepId });

  const engine = ctx.engine!;
  const stream = engine.executeStep(input, signal);
  for await (const event of stream) {
    emitToCtx(ctx, {
      type: "engine_event",
      agentId,
      taskId,
      stepId,
      event,
    });
  }

  const result = await stream.result();
  emitToCtx(ctx, {
    type: "engine_step_completed",
    agentId,
    taskId,
    stepId,
    status: result.status,
  });

  // Append transcript
  if (result.appendedMessages.length > 0) {
    ctx.state.agents = {
      ...ctx.state.agents,
      [agentId]: {
        ...agent,
        transcript: [...agent.transcript, ...result.appendedMessages],
        engineState: result.engineState,
      },
    };
  }

  // Handle result
  switch (result.status) {
    case "completed":
      completeTask(ctx, taskId, { summary: "Task completed" });
      break;
    case "awaiting_approval":
      // Caller (orchestrator) tracks pending approvals; we just return the result
      break;
    case "error":
    case "aborted":
      failTask(ctx, taskId, result.stopReason ?? "Engine error");
      break;
    // "continue" — agent stays running, next tick picks it up
  }

  return result;
}

// ---- Concurrency pool ----

async function pooledMap<T>(
  items: T[],
  concurrency: number,
  fn: (item: T) => Promise<void>,
): Promise<void> {
  const limit = Math.max(1, concurrency);
  let idx = 0;

  async function worker(): Promise<void> {
    while (idx < items.length) {
      const i = idx++;
      await fn(items[i]);
    }
  }

  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, () => worker()));
}

// ---- Tick executor (pure function, no class) ----

/**
 * Execute one engine step for each running agent (concurrently, up to
 * maxConcurrentSteps). Handles approval tracking via side-channel map.
 */
export async function executeAgentSteps(
  ctx: OrchestratorCtx,
  signal?: AbortSignal,
  pendingApprovals?: Map<string, PendingApproval>,
): Promise<void> {
  const running: AgentRuntimeState[] = [];
  for (const a of Object.values(ctx.state.agents)) {
    if (a.status === "running" && a.activeTaskId && !pendingApprovals?.has(a.id)) {
      running.push(a);
    }
  }

  if (running.length === 0) return;

  const concurrency = ctx.engineConfig?.maxConcurrentSteps ?? running.length;

  await pooledMap(running, concurrency, async (agent) => {
    if (signal?.aborted) return;

    const taskId = agent.activeTaskId!;
    const result = await runAndHandleStep(ctx, agent.id, taskId, signal);

    if (result.status === "awaiting_approval" && result.pendingApproval && pendingApprovals) {
      pendingApprovals.set(agent.id, {
        approvalId: result.pendingApproval.requestId,
        taskId,
        details: result.pendingApproval.details,
        engineState: result.engineState,
      });
      emitToCtx(ctx, {
        type: "approval_requested",
        agentId: agent.id,
        taskId,
        approvalId: result.pendingApproval.requestId,
        details: result.pendingApproval.details,
      });
    }
  });
}
