// ---- AgentActor — model step execution + terminal helpers ----

import type {
  EventStream,
  ModelProviderConfig,
  ModelRunSettings,
  ToolDef,
} from "piko-orchestrator-protocol";
import type { ActorContext } from "../../kernel/actor-system.js";
import type {
  ModelStepEvent,
  ModelStepExecutor,
  ModelStepInput,
  ModelStepResult,
} from "../../model/types.js";
import type { CatalogRoute } from "../tool.js";
import { executeToolCalls } from "./tool-executor.js";
import type { AgentActorDeps, AgentRuntimeState, StepOutcome } from "./types.js";

/** Call the model executor with the current transcript, stream deltas via emit. */
export async function runModelStep(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  executor: ModelStepExecutor,
  model: import("piko-orchestrator-protocol").Model<string>,
  provider: ModelProviderConfig,
  settings: ModelRunSettings,
  tools: ToolDef[],
  taskId: string,
): Promise<ModelStepResult> {
  const input: ModelStepInput = {
    runId: taskId,
    stepId: `step_${state.stepCount}`,
    transcript: state.transcript,
    systemPrompt: state.spec.systemPrompt,
    model,
    provider,
    settings,
    tools,
    engineState: state.engineState,
  };

  const stream: EventStream<ModelStepEvent, ModelStepResult> = executor.executeStep(input);

  for await (const event of stream) {
    switch (event.type) {
      case "message_delta":
        await deps.emit({
          type: "task_delta",
          agentId: state.spec.id,
          taskId,
          delta: { kind: "text", text: event.delta },
        });
        break;
      case "thinking_delta":
        await deps.emit({
          type: "task_delta",
          agentId: state.spec.id,
          taskId,
          delta: { kind: "thinking", text: event.delta },
        });
        break;
      case "message_end":
        state.transcript.push(event.message);
        break;
      case "step_start":
      case "step_end":
      case "error":
        break;
    }
  }

  const stepResult = await stream.result();

  // Merge appended messages into transcript
  for (const msg of stepResult.appendedMessages ?? []) {
    if (!state.transcript.includes(msg)) {
      state.transcript.push(msg);
    }
  }

  state.engineState = stepResult.engineState;
  return stepResult;
}

/** Process the step result: handle error/abort, extract tool calls, execute them. */
export async function processStepOutcome(
  state: AgentRuntimeState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  taskId: string,
  stepResult: ModelStepResult,
  modelSettings: ModelRunSettings,
  routes: Map<string, CatalogRoute>,
): Promise<StepOutcome> {
  // ---- Terminal: error / aborted ----
  if (stepResult.status === "error" || stepResult.status === "aborted") {
    const summary = stepResult.status === "error" ? "Unknown engine error" : "Task aborted";
    return terminalStep(state, summary, stepResult.status);
  }

  // ---- Extract assistant message ----
  const assistantMessage = stepResult.appendedMessages?.find((m) => m.role === "assistant");
  if (!assistantMessage) {
    return { kind: "continue" };
  }

  const toolCalls = (
    Array.isArray(assistantMessage.content)
      ? assistantMessage.content.filter(
          (c: unknown) => (c as { type?: string }).type === "toolCall",
        )
      : []
  ) as Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>;

  // ---- No tool calls: task completed ----
  if (toolCalls.length === 0 || !modelSettings.allowToolCalls) {
    const text =
      typeof assistantMessage.content === "string"
        ? assistantMessage.content
        : JSON.stringify(assistantMessage.content);
    const summary = text.slice(0, 200);

    await deps.emit({
      type: "task_completed",
      agentId: state.spec.id,
      taskId,
      result: { summary },
    });

    return terminalStep(state, summary, "completed");
  }

  // ---- Execute tool calls (parallel or sequential) ----
  await executeToolCalls(state, deps, ctx, taskId, toolCalls, modelSettings, routes);
  return { kind: "continue" };
}

/** Mark agent idle and return a terminal outcome. */
export function terminalStep(
  state: AgentRuntimeState,
  summary: string,
  finalStatus: string,
): StepOutcome {
  state.status = "idle";
  state.currentTaskId = undefined;
  return {
    kind: "terminal",
    result: {
      summary,
      messages: state.transcript,
      totalSteps: state.stepCount,
      finalStatus,
    },
  };
}
