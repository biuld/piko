import type { EngineEvent, EngineInput, EngineStepResult } from "piko-engine-protocol";
import { piAiAdapter as defaultAdapter } from "../provider/pi-ai-adapter.js";
import type { ProviderAdapter } from "../provider/types.js";
import { runProviderCall } from "../provider-runner.js";
import { checkBeforeModelCall } from "../runtime-limits.js";
import { prepareToolCalls } from "../tool-runner.js";
import type { NativeToolRegistry } from "../types.js";
import {
  buildContinuationState,
  createReadyContinuationState,
  getOrCreateCounters,
} from "./continuation-state.js";
import { buildTranscriptDelta } from "./transcript-delta.js";

/**
 * Run one step: provider call → prepare tool calls (no execution).
 * Engine returns `awaiting_resource` with pending tools; caller executes them
 * and calls engine.resolveResource() to continue.
 */
export async function runStepStateMachine(
  input: EngineInput,
  _registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
  adapter?: ProviderAdapter,
): Promise<EngineStepResult> {
  const { settings, tools } = input;
  const effectiveTools = tools ?? [];

  if (signal?.aborted) {
    return {
      status: "aborted",
      appendedMessages: [],
      transcriptDelta: [],
      stopReason: "abort",
      engineState: input.engineState,
    };
  }

  const counters = getOrCreateCounters(input);

  const limitCheck = checkBeforeModelCall(counters, settings.runtimeLimits);
  if (limitCheck?.exceeded) {
    return {
      status: "completed",
      appendedMessages: [],
      transcriptDelta: [],
      stopReason: limitCheck.stopReason as "max_steps" | "abort" | "error",
      engineState: createReadyContinuationState(counters),
    };
  }

  emit({ type: "step_start" });
  counters.modelCalls++;

  // ---- Phase 1: Provider call ----
  const providerResult = await runProviderCall(input, emit, signal, adapter ?? defaultAdapter);
  const assistantMessage = providerResult.assistantMessage;

  if (providerResult.isError || assistantMessage.role !== "assistant") {
    counters.consecutiveErrors++;
    emit({ type: "step_end" });
    return {
      status: "error",
      appendedMessages: [assistantMessage],
      transcriptDelta: buildTranscriptDelta([assistantMessage]),
      stopReason: "error",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  const appendedMessages = [assistantMessage];

  // Check stop conditions
  if (settings.stopConditions?.stopOnAssistantMessage) {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: providerResult.tokenUsage,
      stopReason: "assistant",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  // ---- Phase 2: Prepare tool calls (no execution) ----
  const toolCalls = Array.isArray(assistantMessage.content)
    ? assistantMessage.content.filter((c: unknown) => (c as { type?: string }).type === "toolCall")
    : [];

  if (!settings.allowToolCalls || toolCalls.length === 0) {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: providerResult.tokenUsage,
      stopReason: "assistant",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  const toolPrep = prepareToolCalls(assistantMessage, effectiveTools);

  if (toolPrep.kind === "completed") {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages: [...appendedMessages, ...toolPrep.messages],
      transcriptDelta: buildTranscriptDelta([...appendedMessages, ...toolPrep.messages]),
      stopReason: "tool",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  // ---- Phase 3: Await resource resolution ----
  const continuationState = buildContinuationState(input, assistantMessage, counters, {
    pendingToolSnapshot: toolPrep.pendingToolSnapshot,
  });

  emit({
    type: "resource_requested",
    request: {
      assistantMessage,
      remainingToolCallIds: toolPrep.pendingToolSnapshot!.remainingToolCalls.map((tc) => tc.id),
      toolCalls: toolPrep.pendingToolSnapshot!.remainingToolCalls.map((tc) => ({
        id: tc.id,
        name: tc.name,
        args: tc.arguments,
        executorTarget: tc.executorTarget,
        executionMode: tc.executionMode,
      })),
      settings,
    },
  });

  emit({ type: "step_end" });

  return {
    status: "awaiting_resource",
    appendedMessages: [...appendedMessages, ...toolPrep.messages],
    transcriptDelta: buildTranscriptDelta([...appendedMessages, ...toolPrep.messages]),
    usage: providerResult.tokenUsage,
    pendingTools: continuationState.pendingToolCalls!,
    stopReason: "resource",
    engineState: continuationState,
  };
}
