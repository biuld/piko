// ---- AgentActor — main engine loop ----

import type { AgentTask, AgentTaskResult, Message, ToolDef } from "piko-orchestrator-protocol";
import type { ActorContext } from "../../kernel/actor-system.js";
import type { CatalogRoute } from "../tool.js";
import { processStepOutcome, runModelStep } from "./step-runner.js";
import type { AgentActorDeps, AgentRuntimeState, StepTerminal } from "./types.js";

/** Main model step loop: discover → call model → process outcome → repeat. */
export async function runEngineLoop(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  task: AgentTask,
): Promise<AgentTaskResult & { messages: Message[]; totalSteps: number; finalStatus: string }> {
  const maxSteps = deps.maxSteps || state.spec.maxSteps || 50;
  const executor = deps.modelExecutor;
  const modelSettings = deps.modelConfig?.settings ?? {
    maxSteps: 1,
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

  while (state.stepCount < maxSteps) {
    const cancelled = checkCancelled(state, taskId);
    if (cancelled) return cancelled;

    state.stepCount++;

    const { tools, routes } = await discoverTools(state, deps, taskId);

    const stepResult = await runModelStep(
      state,
      deps,
      executor,
      model,
      provider,
      modelSettings,
      tools,
      taskId,
    );

    const outcome = await processStepOutcome(
      state,
      deps,
      ctx,
      taskId,
      stepResult,
      modelSettings,
      routes,
    );
    if (outcome.kind === "terminal") return outcome.result;
  }

  return buildMaxStepsResult(state, deps, taskId, maxSteps);
}

function checkCancelled(state: AgentRuntimeState, taskId: string): StepTerminal | null {
  if (!state.cancelled.has(taskId)) return null;

  state.status = "idle";
  state.cancelled.delete(taskId);
  return {
    summary: "Task cancelled",
    messages: state.transcript,
    totalSteps: state.stepCount,
    finalStatus: "aborted",
  };
}

async function discoverTools(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  taskId: string,
): Promise<{ tools: ToolDef[]; routes: Map<string, CatalogRoute> }> {
  try {
    return await deps.toolRegistry.discoverTools({
      agentId: state.spec.id,
      taskId,
      toolSetIds: state.spec.toolSetIds,
      activeToolNames: state.spec.activeToolNames,
    });
  } catch {
    return { tools: [], routes: new Map() };
  }
}

/** Build the max-steps-reached terminal result. */
function buildMaxStepsResult(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  taskId: string,
  maxSteps: number,
): StepTerminal {
  state.status = "idle";
  state.currentTaskId = undefined;

  const finalMsg =
    state.transcript
      .filter((m: Message) => m.role === "assistant")
      .map((m: Message) => (typeof m.content === "string" ? m.content : JSON.stringify(m.content)))
      .join("\n") || "Max steps reached";

  const error = `Max steps (${maxSteps}) reached.`;
  deps.emit({ type: "task_failed", agentId: state.spec.id, taskId, error }).catch(() => {});

  return {
    summary: `${error} ${finalMsg}`,
    messages: state.transcript,
    totalSteps: state.stepCount,
    finalStatus: "max_steps",
  };
}
