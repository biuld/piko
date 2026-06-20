// ---- AgentActor — tool execution (parallel / sequential) ----

import {
  type DebugTraceInput,
  debugTrace,
  type Message,
  type ModelRunSettings,
  startDebugSpan,
  type ToolExecResult,
  type ToolExecutionContext,
} from "piko-orchestrator-protocol";
import type { ActorContext } from "../../kernel/actor-system.js";
import type { CatalogRoute } from "../../tools/tool-registry.js";
import type { AgentActorDeps, AgentRuntimeState, AgentWorkerState } from "./types.js";

function createAbortError(): Error {
  const error = new Error("Task cancelled");
  error.name = "AbortError";
  return error;
}

/**
 * Stop awaiting an operation as soon as cancellation is requested, even when
 * the underlying provider ignores the AbortSignal. The provider promise is
 * still observed so a late rejection cannot become unhandled.
 */
export function raceWithAbort<T>(
  operation: Promise<T>,
  signal?: AbortSignal,
  onLateSettlement?: (outcome: "completed" | "error") => void,
): Promise<T> {
  if (!signal) return operation;
  if (signal.aborted) return Promise.reject(createAbortError());

  return new Promise<T>((resolve, reject) => {
    let settled = false;

    const finish = (callback: () => void) => {
      if (settled) return;
      settled = true;
      signal.removeEventListener("abort", onAbort);
      callback();
    };
    const onAbort = () => finish(() => reject(createAbortError()));

    signal.addEventListener("abort", onAbort, { once: true });
    // Cover cancellation between the initial check and listener registration.
    if (signal.aborted) onAbort();

    operation.then(
      (value) => {
        if (settled) onLateSettlement?.("completed");
        finish(() => resolve(value));
      },
      (error) => {
        if (settled) onLateSettlement?.("error");
        finish(() => reject(error));
      },
    );
  });
}

async function executeOneTool(
  deps: AgentActorDeps,
  tc: { id: string; name: string; arguments: Record<string, unknown> },
  execContext: ToolExecutionContext,
  route: CatalogRoute,
  signal?: AbortSignal,
): Promise<ToolExecResult> {
  const fields: Partial<DebugTraceInput> = {
    taskId: execContext.taskId,
    agentId: execContext.agentId,
    toolCallId: tc.id,
    toolName: tc.name,
    eventSeq: execContext.eventSeq,
    signalAborted: signal?.aborted ?? false,
  };
  const span = startDebugSpan("tool.execute", fields);
  try {
    const result = await raceWithAbort(
      deps.toolRegistry.executeTool(
        { type: "toolCall", id: tc.id, name: tc.name, arguments: tc.arguments },
        execContext,
        route,
        signal,
      ),
      signal,
      (outcome) => debugTrace({ ...fields, stage: "tool.execute.late_settlement", outcome }),
    );
    span.end({ outcome: result.error ? "error" : "completed" });
    return result;
  } catch (error) {
    span.end({
      outcome:
        signal?.aborted || (error instanceof Error && error.name === "AbortError")
          ? "aborted"
          : "error",
      signalAborted: signal?.aborted ?? false,
    });
    throw error;
  }
}

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
 * Parallel path: executes all tool calls concurrently via Promise.all.
 * Sequential path: executes each tool call sequentially in a for...of loop.
 *
 * Results are always appended to transcript in the original tool call order.
 * Ordering metadata (turnIndex, parentMessageId, contentIndex, toolCallIndex)
 * is passed through the ToolExecutionContext for the tool-registry to emit.
 */
export async function executeToolCalls(
  state: AgentRuntimeState,
  workerState: AgentWorkerState,
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
  signal?: AbortSignal,
  parentMessageId?: string,
  toolCallOrder?: Map<string, { contentIndex: number; toolCallIndex: number }>,
): Promise<void> {
  const mode = resolveExecutionMode(toolCalls, routes, modelSettings);

  if (mode === "parallel") {
    return executeParallel(
      state,
      workerState,
      deps,
      ctx,
      taskId,
      toolCalls,
      routes,
      signal,
      parentMessageId,
      toolCallOrder,
    );
  }

  return executeSequential(
    state,
    workerState,
    deps,
    ctx,
    taskId,
    toolCalls,
    routes,
    signal,
    parentMessageId,
    toolCallOrder,
  );
}

/** Build execution context with ordering metadata. */
function makeContext(
  state: AgentRuntimeState,
  taskId: string,
  tc: { id: string },
  turnIndex: number,
  eventSeq: number,
  nextEventSeq: () => number,
  parentMessageId?: string,
  toolCallOrder?: Map<string, { contentIndex: number; toolCallIndex: number }>,
): ToolExecutionContext {
  const order = toolCallOrder?.get(tc.id);
  return {
    agentId: state.spec.id,
    taskId,
    toolSetIds: state.spec.toolSetIds,
    turnIndex,
    eventSeq,
    nextEventSeq,
    parentMessageId: parentMessageId ?? "",
    contentIndex: order?.contentIndex ?? 0,
    toolCallIndex: order?.toolCallIndex ?? 0,
  };
}

/** Parallel execution: runs all tool calls concurrently. */
async function executeParallel(
  state: AgentRuntimeState,
  workerState: AgentWorkerState,
  deps: AgentActorDeps,
  _ctx: ActorContext,
  taskId: string,
  toolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>,
  routes: Map<string, CatalogRoute>,
  signal?: AbortSignal,
  parentMessageId?: string,
  toolCallOrder?: Map<string, { contentIndex: number; toolCallIndex: number }>,
): Promise<void> {
  // Fire all execution requests concurrently
  const promises = toolCalls.map(async (tc, index) => {
    const route = routes.get(tc.name);
    if (!route) {
      return { index, tc, error: `No route for tool "${tc.name}"` };
    }

    if (signal?.aborted) {
      return { index, tc, error: "Task cancelled" };
    }

    const execContext = makeContext(
      state,
      taskId,
      tc,
      workerState.stepCount,
      workerState.eventSeq,
      () => ++workerState.eventSeq,
      parentMessageId,
      toolCallOrder,
    );

    try {
      const execResult = await executeOneTool(deps, tc, execContext, route, signal);
      return { index, tc, result: execResult };
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
      workerState.transcript.push({
        role: "toolResult",
        toolName: item.tc.name,
        toolCallId: item.tc.id,
        content: [{ type: "text", text: `Tool error: ${item.error}` }],
        details: { error: item.error },
        isError: true,
        timestamp: Date.now(),
      } as Message);
    } else {
      appendToolResult(workerState, item.tc, item.result as ToolExecResult);
    }
  }
}

async function executeSequential(
  state: AgentRuntimeState,
  workerState: AgentWorkerState,
  deps: AgentActorDeps,
  _ctx: ActorContext,
  taskId: string,
  toolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>,
  routes: Map<string, CatalogRoute>,
  signal?: AbortSignal,
  parentMessageId?: string,
  toolCallOrder?: Map<string, { contentIndex: number; toolCallIndex: number }>,
): Promise<void> {
  for (const tc of toolCalls) {
    const execContext = makeContext(
      state,
      taskId,
      tc,
      workerState.stepCount,
      workerState.eventSeq,
      () => ++workerState.eventSeq,
      parentMessageId,
      toolCallOrder,
    );

    if (signal?.aborted) {
      workerState.transcript.push({
        role: "toolResult",
        toolName: tc.name,
        toolCallId: tc.id,
        content: [{ type: "text", text: "Tool error: Task cancelled" }],
        details: { error: "Task cancelled" },
        isError: true,
        timestamp: Date.now(),
      } as Message);
      continue;
    }

    const route = routes.get(tc.name);
    if (!route) {
      workerState.transcript.push({
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
      const execResult = await executeOneTool(deps, tc, execContext, route, signal);
      appendToolResult(workerState, tc, execResult);
    } catch (err) {
      const errorText = err instanceof Error ? err.message : String(err);
      workerState.transcript.push({
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
}

/** Append a successful tool execution result to the transcript. */
export function appendToolResult(
  workerState: AgentWorkerState,
  tc: { id: string; name: string },
  execResult: ToolExecResult,
): void {
  const text =
    typeof execResult.value === "string"
      ? execResult.value
      : JSON.stringify(execResult.ok ? execResult.value : execResult.error, null, 2);

  workerState.transcript.push({
    role: "toolResult",
    toolName: tc.name,
    toolCallId: tc.id,
    content: [{ type: "text", text }],
    details: execResult.ok ? execResult.value : execResult.error,
    isError: !execResult.ok,
    timestamp: Date.now(),
  } as Message);
}
