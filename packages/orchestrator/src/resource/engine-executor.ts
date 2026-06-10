// ---- Engine step executor: runs engine for all running agents ----

import type { EngineInput, EngineStepResult } from "piko-engine-protocol";
import type { OrchestratorCtx } from "../context.js";
import { emitToCtx } from "../context.js";
import { completeTask, failTask } from "../task/index.js";

/**
 * Run one engine step for an agent and handle the result
 * (append transcript, complete/fail/resource based on status).
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

  const engine = ctx.engine!;
  const stream = engine.executeStep(input, signal);
  // Engine events are consumed internally — not emitted as orchestrator events.
  for await (const _event of stream) {
    // Engine events are internal to the orchestrator.
    // In the future, bridge to AgentEvent for TUI streaming.
  }

  const result = await stream.result();

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

    case "awaiting_resource": {
      // Emit a unified resource.requested event
      if (result.pendingTools) {
        const items = result.pendingTools.toolCalls.map((tc) => ({
          kind: "tool" as const,
          id: tc.id,
          name: tc.name,
          args: tc.args,
        }));
        emitToCtx(ctx, {
          subsystem: "resource",
          type: "requested",
          taskId,
          agentId,
          items,
        });
      }
      break;
    }

    case "error":
    case "aborted":
      failTask(ctx, taskId, result.stopReason ?? "Engine error");
      break;
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

// ---- Tick executor ----

/**
 * Execute one engine step for each running agent (concurrently, up to
 * maxConcurrentSteps).
 */
export async function executeAgentSteps(ctx: OrchestratorCtx, signal?: AbortSignal): Promise<void> {
  const running = Object.values(ctx.state.agents).filter(
    (a) => a.status === "running" && a.activeTaskId,
  );

  if (running.length === 0) return;

  const concurrency = ctx.engineConfig?.maxConcurrentSteps ?? running.length;

  await pooledMap(running, concurrency, async (agent) => {
    if (signal?.aborted) return;
    await runAndHandleStep(ctx, agent.id, agent.activeTaskId!, signal);
  });
}
