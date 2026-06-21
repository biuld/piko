// ---- AgentActor — model step execution + terminal helpers ----

import {
  type EventStream,
  type Message,
  type ModelProviderConfig,
  type ModelRunSettings,
  type RuntimeMessage,
  type ToolDef,
  toRuntimeMessage,
} from "piko-orchestrator-protocol";

import type { ActorContext } from "../../kernel/actor-system.js";
import type {
  ModelStepEvent,
  ModelStepExecutor,
  ModelStepInput,
  ModelStepResult,
} from "../../model/types.js";
import { runtimeAssistantMessageId } from "../../model/types.js";
import type { CatalogRoute } from "../../tools/tool-registry.js";
import { executeToolCalls } from "./tool-executor.js";
import type { AgentActorDeps, AgentRuntimeState, AgentWorkerState, StepOutcome } from "./types.js";

/** Call the model executor with the current transcript, stream deltas via emit. */
export async function runModelStep(
  state: AgentRuntimeState,
  workerState: AgentWorkerState,
  deps: AgentActorDeps,
  executor: ModelStepExecutor,
  model: import("piko-orchestrator-protocol").Model<string>,
  provider: ModelProviderConfig,
  settings: ModelRunSettings,
  tools: ToolDef[],
  taskId: string,
  signal?: AbortSignal,
): Promise<{ stepResult: ModelStepResult; assistantMessageId?: string }> {
  const input: ModelStepInput = {
    runId: taskId,
    stepId: `step_${workerState.stepCount}`,
    transcript: workerState.transcript,
    systemPrompt: state.spec.systemPrompt,
    model,
    provider,
    settings,
    tools,
    engineState: workerState.engineState,
  };

  const stream: EventStream<ModelStepEvent, ModelStepResult> = executor.executeStep(input, signal);

  // Track the assistant message ID for tool parent resolution
  let assistantMessageId: string | undefined;

  for await (const event of stream) {
    if (signal?.aborted) {
      break;
    }

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
      case "message_start": {
        const msgIndex = workerState.nextMessageIndex++;
        const seq = ++workerState.eventSeq;
        workerState.messageIndexById.set(event.message.id, msgIndex);
        // Capture the runtime message ID for tool parent resolution
        assistantMessageId = event.message.id;
        await deps.emit({
          type: "task_message_start",
          agentId: state.spec.id,
          taskId,
          eventSeq: seq,
          turnIndex: workerState.stepCount,
          messageIndex: msgIndex,
          message: event.message,
        });
        break;
      }
      case "message_update":
        await deps.emit({
          type: "task_message_update",
          agentId: state.spec.id,
          taskId,
          eventSeq: ++workerState.eventSeq,
          turnIndex: workerState.stepCount,
          messageIndex: workerState.messageIndexById.get(event.message.id),
          message: event.message,
          assistantEvent: event.assistantEvent,
        });
        break;
      case "message_end": {
        const isRuntime =
          "role" in event.message &&
          typeof (event.message as any).id === "string" &&
          Array.isArray((event.message as any).content) &&
          typeof (event.message as any).content[0] === "object";
        const stableId = runtimeAssistantMessageId(taskId, `step_${workerState.stepCount}`);
        const runtimeMsg = (
          isRuntime ? event.message : toRuntimeMessage(event.message as Message, stableId)
        ) as RuntimeMessage;
        const msgIdxForEnd =
          workerState.messageIndexById.get(runtimeMsg.id) ??
          workerState.messageIndexById.get(assistantMessageId ?? "");
        assistantMessageId = runtimeMsg.id;
        await deps.emit({
          type: "task_message_end",
          agentId: state.spec.id,
          taskId,
          eventSeq: ++workerState.eventSeq,
          turnIndex: workerState.stepCount,
          messageIndex: msgIdxForEnd,
          message: runtimeMsg,
        });
        break;
      }
      case "step_start":
      case "step_end":
      case "error":
        break;
    }
  }

  if (signal?.aborted) {
    return {
      stepResult: {
        status: "aborted",
        appendedMessages: [],
        stopReason: "abort",
        engineState: workerState.engineState,
      },
      assistantMessageId,
    };
  }

  const stepResult = await stream.result();

  // Merge appended messages into transcript
  for (const msg of stepResult.appendedMessages ?? []) {
    const exists = workerState.transcript.some(
      (t) =>
        t === msg ||
        (t.role === msg.role && JSON.stringify(t.content) === JSON.stringify(msg.content)),
    );
    if (!exists) {
      workerState.transcript.push(msg);
    }
  }

  workerState.engineState = stepResult.engineState;
  // Return the actual runtime message ID alongside the result
  return { stepResult, assistantMessageId };
}

/** Process the step result: handle error/abort, extract tool calls, execute them. */
export async function processStepOutcome(
  state: AgentRuntimeState,
  workerState: AgentWorkerState,
  deps: AgentActorDeps,
  ctx: ActorContext,
  taskId: string,
  stepResult: ModelStepResult,
  modelSettings: ModelRunSettings,
  routes: Map<string, CatalogRoute>,
  signal?: AbortSignal,
  assistantMessageId?: string,
): Promise<StepOutcome> {
  // ---- Terminal: error / aborted ----
  if (stepResult.status === "error" || stepResult.status === "aborted" || signal?.aborted) {
    const status = signal?.aborted || stepResult.status === "aborted" ? "aborted" : "error";
    const summary = status === "error" ? "Unknown engine error" : "Task cancelled";
    return terminalStep(state, workerState, summary, status);
  }

  // ---- Extract assistant message ----
  const assistantMessage = stepResult.appendedMessages?.find((m) => m.role === "assistant");
  if (!assistantMessage) {
    return { kind: "continue" };
  }

  const contentBlocks = Array.isArray(assistantMessage.content) ? assistantMessage.content : [];
  const toolCalls = contentBlocks.filter(
    (c: unknown) => (c as { type?: string }).type === "toolCall",
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

    return terminalStep(state, workerState, summary, signal?.aborted ? "aborted" : "completed");
  }

  // ---- Assign tool ordering metadata from assistant message content ----
  // Build maps: toolCallId -> { contentIndex, toolCallIndex }
  // Use the actual runtime message ID from the model caller (captured at message_start)
  const parentMessageId =
    assistantMessageId ?? runtimeAssistantMessageId(taskId, `step_${workerState.stepCount}`);
  let toolCallSeq = 0;
  const toolCallOrder = new Map<string, { contentIndex: number; toolCallIndex: number }>();
  for (let i = 0; i < contentBlocks.length; i++) {
    const block = contentBlocks[i];
    if (block.type === "toolCall") {
      toolCallOrder.set(block.id, { contentIndex: i, toolCallIndex: toolCallSeq++ });
    }
  }

  // ---- Execute tool calls (parallel or sequential) ----
  if (signal?.aborted) {
    return terminalStep(state, workerState, "Task cancelled", "aborted");
  }

  await executeToolCalls(
    state,
    workerState,
    deps,
    ctx,
    taskId,
    toolCalls,
    modelSettings,
    routes,
    signal,
    parentMessageId,
    toolCallOrder,
  );
  return { kind: "continue" };
}

/** Mark agent idle and return a terminal outcome. */
export function terminalStep(
  _state: AgentRuntimeState,
  workerState: AgentWorkerState,
  summary: string,
  finalStatus: string,
): StepOutcome {
  return {
    kind: "terminal",
    result: {
      summary,
      messages: workerState.transcript,
      totalSteps: workerState.stepCount,
      finalStatus,
    },
  };
}
