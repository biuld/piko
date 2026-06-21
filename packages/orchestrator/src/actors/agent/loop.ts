// ---- AgentActor — main engine loop ----

import {
  type AgentTask,
  type AgentTaskResult,
  type Message,
  startDebugSpan,
  type ToolDef,
} from "piko-orchestrator-protocol";
import type { ActorContext } from "../../kernel/actor-system.js";
import type { CatalogRoute } from "../../tools/tool-registry.js";
import { processStepOutcome, runModelStep } from "./step-runner.js";
import type { AgentActorDeps, AgentRuntimeState, AgentWorkerState, StepTerminal } from "./types.js";

/** Main model step loop: discover → call model → process outcome → repeat. */
export async function runEngineLoop(
  state: AgentRuntimeState,
  workerState: AgentWorkerState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  task: AgentTask,
  signal?: AbortSignal,
): Promise<AgentTaskResult & { messages: Message[]; totalSteps: number; finalStatus: string }> {
  const executor = deps.modelExecutor;
  const modelSettings = deps.modelConfig?.settings ?? {
    allowToolCalls: true,
  };
  const model =
    deps.modelConfig?.model ??
    ({
      id: "default",
      name: "Default",
    } as import("piko-orchestrator-protocol").Model<string>);
  const provider = deps.modelConfig?.provider ?? {};
  const taskId = task.id ?? "unknown";

  while (true) {
    if (signal?.aborted) {
      return buildCancelledResult(workerState);
    }

    workerState.stepCount++;

    const { tools, routes } = await discoverTools(state, deps, taskId);

    if (signal?.aborted) {
      return buildCancelledResult(workerState);
    }

    const { stepResult, assistantMessageId } = await runModelStep(
      state,
      workerState,
      deps,
      executor,
      model,
      provider,
      modelSettings,
      tools,
      taskId,
      signal,
    );

    if (signal?.aborted) {
      return buildCancelledResult(workerState);
    }

    const outcome = await processStepOutcome(
      state,
      workerState,
      deps,
      ctx,
      taskId,
      stepResult,
      modelSettings,
      routes,
      signal,
      assistantMessageId,
    );
    if (outcome.kind === "terminal") return outcome.result;
  }
}

function buildCancelledResult(workerState: AgentWorkerState): StepTerminal {
  return {
    summary: "Task cancelled",
    messages: workerState.transcript,
    totalSteps: workerState.stepCount,
    finalStatus: "aborted",
  };
}

async function discoverTools(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  taskId: string,
): Promise<{ tools: ToolDef[]; routes: Map<string, CatalogRoute> }> {
  const span = startDebugSpan("tool.discover", { taskId, agentId: state.spec.id });
  try {
    const result = await deps.toolRegistry.discoverTools({
      agentId: state.spec.id,
      taskId,
      toolSetIds: state.spec.toolSetIds,
      activeToolNames: state.spec.activeToolNames,
    });
    span.end({ outcome: "completed", count: result.tools.length });
    return result;
  } catch (_err) {
    span.end({ outcome: "error" });
    return { tools: [], routes: new Map() };
  }
}
