// ---- AgentActor — tool execution (parallel / sequential) ----

import type { Message, ModelRunSettings, ToolExecResult } from "piko-orchestrator-protocol";
import type { ActorContext } from "../../kernel/actor-system.js";
import type { CatalogRoute } from "../tool.js";
import type { AgentActorDeps, AgentRuntimeState } from "./types.js";

// ---- Execution mode resolution ----

/**
 * Determine whether the batch of tool calls should run in parallel or sequentially.
 *
 * Rules:
 * 1. If settings.parallelTools === false → sequential (config override)
 * 2. If any tool in the batch has executionMode === "sequential" → sequential
 * 3. Otherwise → parallel (default)
 */
export function resolveExecutionMode(
  toolCalls: Array<{ name: string }>,
  routes: Map<string, CatalogRoute>,
  settings: ModelRunSettings,
): "parallel" | "sequential" {
  if (settings.parallelTools === false) return "sequential";

  for (const tc of toolCalls) {
    const route = routes.get(tc.name);
    if (route?.toolDef?.executionMode === "sequential") return "sequential";
  }

  return "parallel";
}

// ---- Tool execution ----

/**
 * Execute a batch of tool calls.
 * Parallel path: spawns one ToolActor per call, runs all concurrently via Promise.all.
 * Sequential path: spawns one ToolActor, runs each call in a for...of loop.
 *
 * Results are always appended to transcript in the original tool call order.
 * Cleanup: all spawned ToolActors are stopped via Promise.all.
 */
export async function executeToolCalls(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  taskId: string,
  toolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>,
  modelSettings: ModelRunSettings,
  routes: Map<string, CatalogRoute>,
): Promise<void> {
  const mode = resolveExecutionMode(toolCalls, routes, modelSettings);

  if (mode === "parallel") {
    return executeParallel(state, deps, ctx, taskId, toolCalls, routes);
  }

  return executeSequential(state, deps, ctx, taskId, toolCalls, routes);
}

/** Parallel execution: one ToolActor per tool call, all running concurrently. */
async function executeParallel(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  taskId: string,
  toolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>,
  routes: Map<string, CatalogRoute>,
): Promise<void> {
  if (!deps.actorSystem) return;

  const toolIds: string[] = [];

  // Fire all execution requests concurrently
  const promises = toolCalls.map(async (tc, index) => {
    const route = routes.get(tc.name);
    if (!route) {
      return { index, tc, error: `No route for tool "${tc.name}"` };
    }

    const tid = deps.toolRegistry.spawnToolActor(
      `tool:${state.spec.id}:step_${state.stepCount}:call_${index}`,
    );
    toolIds.push(tid);

    try {
      const execResult = await deps.actorSystem!.ask<ToolExecResult>(
        tid,
        {
          type: "execute",
          call: { id: tc.id, name: tc.name, arguments: tc.arguments },
          context: {
            agentId: state.spec.id,
            taskId,
            toolSetIds: state.spec.toolSetIds,
          },
          route,
        },
        ctx.self.id,
      );
      return { index, tc, result: execResult as ToolExecResult | { error: string } };
    } catch (err) {
      return {
        index,
        tc,
        result: { error: err instanceof Error ? err.message : String(err) },
      };
    }
  });

  const results = await Promise.all(promises);

  // Append results in original order
  results.sort((a, b) => a.index - b.index);
  for (const item of results) {
    if ("error" in item) {
      state.transcript.push({
        role: "toolResult",
        toolName: item.tc.name,
        toolCallId: item.tc.id,
        content: [{ type: "text", text: `Tool error: ${item.error}` }],
        details: { error: item.error },
        isError: true,
        timestamp: Date.now(),
      } as Message);
    } else {
      appendToolResult(state, item.tc, item.result as ToolExecResult);
    }
  }

  // Cleanup all spawned ToolActors
  await Promise.all(toolIds.map((id) => deps.toolRegistry.stopToolActor(id).catch(() => {})));
}

/** Sequential execution: one ToolActor processes all calls one at a time. */
async function executeSequential(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  taskId: string,
  toolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>,
  routes: Map<string, CatalogRoute>,
): Promise<void> {
  if (!deps.actorSystem) return;

  const tid = deps.toolRegistry.spawnToolActor(`tool:${state.spec.id}:step_${state.stepCount}`);

  try {
    for (const tc of toolCalls) {
      const route = routes.get(tc.name);
      if (!route) {
        state.transcript.push({
          role: "toolResult",
          toolName: tc.name,
          toolCallId: tc.id,
          content: [{ type: "text", text: `Tool error: No route for tool "${tc.name}"` }],
          details: { error: `No route for tool "${tc.name}"` },
          isError: true,
          timestamp: Date.now(),
        } as Message);
        continue;
      }

      try {
        const execResult = await deps.actorSystem!.ask<ToolExecResult>(
          tid,
          {
            type: "execute",
            call: { id: tc.id, name: tc.name, arguments: tc.arguments },
            context: {
              agentId: state.spec.id,
              taskId,
              toolSetIds: state.spec.toolSetIds,
            },
            route,
          },
          ctx.self.id,
        );
        appendToolResult(state, tc, execResult);
      } catch (err) {
        const errorText = err instanceof Error ? err.message : String(err);
        state.transcript.push({
          role: "toolResult",
          toolName: tc.name,
          toolCallId: tc.id,
          content: [{ type: "text", text: `Tool error: ${errorText}` }],
          details: { error: errorText },
          isError: true,
          timestamp: Date.now(),
        } as Message);
      }
    }
  } finally {
    await deps.toolRegistry.stopToolActor(tid).catch(() => {});
  }
}

/** Append a successful tool execution result to the transcript. */
export function appendToolResult(
  state: AgentRuntimeState,
  tc: { id: string; name: string },
  execResult: ToolExecResult,
): void {
  const text =
    typeof execResult.value === "string"
      ? execResult.value
      : JSON.stringify(execResult.ok ? execResult.value : execResult.error, null, 2);

  state.transcript.push({
    role: "toolResult",
    toolName: tc.name,
    toolCallId: tc.id,
    content: [{ type: "text", text }],
    details: execResult.ok ? execResult.value : execResult.error,
    isError: !execResult.ok,
    timestamp: Date.now(),
  } as Message);
}
