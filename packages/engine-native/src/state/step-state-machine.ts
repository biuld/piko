import type { EngineEvent, EngineInput, EngineStepResult, Message } from "piko-engine-protocol";
import { createPendingApproval } from "../approval-state.js";
import { piAiAdapter as defaultAdapter } from "../provider/pi-ai-adapter.js";
import type { ProviderAdapter } from "../provider/types.js";
import { runProviderCall } from "../provider-runner.js";
import {
  checkBeforeApproval,
  checkBeforeModelCall,
  checkBeforeToolCall,
} from "../runtime-limits.js";
import { executeToolCalls } from "../tool-runner.js";
import type { NativeToolRegistry } from "../types.js";
import {
  buildContinuationState,
  createReadyContinuationState,
  getOrCreateCounters,
} from "./continuation-state.js";
import { buildTranscriptDelta } from "./transcript-delta.js";

export async function runStepStateMachine(
  input: EngineInput,
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
  adapter?: ProviderAdapter,
  externalToolHandler?: (name: string, args: Record<string, unknown>) => Promise<unknown>,
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

  const providerResult = await runProviderCall(input, emit, signal, adapter ?? defaultAdapter);
  const resultMessage = providerResult.assistantMessage;
  const tokenUsage = providerResult.tokenUsage;

  if (providerResult.isError) {
    counters.consecutiveErrors++;
    emit({ type: "step_end" });
    return {
      status: "error",
      appendedMessages: [resultMessage],
      transcriptDelta: buildTranscriptDelta([resultMessage]),
      stopReason: "error",
      engineState: buildContinuationState(input, resultMessage, counters),
    };
  }

  if (resultMessage.role !== "assistant") {
    counters.consecutiveErrors++;
    emit({ type: "step_end" });
    return {
      status: "error",
      appendedMessages: [resultMessage],
      transcriptDelta: buildTranscriptDelta([resultMessage]),
      stopReason: "error",
      engineState: buildContinuationState(input, resultMessage, counters),
    };
  }

  const assistantMessage = resultMessage;
  const appendedMessages: Message[] = [assistantMessage];

  if (settings.stopConditions?.stopOnAssistantMessage) {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: tokenUsage,
      stopReason: "assistant",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  const content = assistantMessage.content;
  const contentBlocks = Array.isArray(content) ? content : [];
  const toolCalls = contentBlocks.filter((c) => c.type === "toolCall");

  if (!settings.allowToolCalls || toolCalls.length === 0) {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: tokenUsage,
      stopReason: "assistant",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  const toolLimitCheck = checkBeforeToolCall(counters, settings.runtimeLimits);
  if (toolLimitCheck?.exceeded) {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: tokenUsage,
      stopReason: toolLimitCheck.stopReason as "max_steps" | "abort" | "error",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  const toolResult = await executeToolCalls(
    assistantMessage,
    effectiveTools,
    registry,
    emit,
    emit,
    settings,
    signal,
    undefined,
    counters,
    undefined,
    externalToolHandler,
  );

  const continuationState = buildContinuationState(
    input,
    assistantMessage,
    counters,
    toolResult.kind === "awaiting_approval" ? toolResult : undefined,
  );

  if (toolResult.kind === "limit_reached") {
    appendedMessages.push(...toolResult.messages);
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: tokenUsage,
      stopReason: toolResult.limitStopReason,
      engineState: continuationState,
    };
  }

  if (toolResult.kind === "awaiting_approval") {
    appendedMessages.push(...toolResult.messages);

    const approvalLimitCheck = checkBeforeApproval(counters, settings.runtimeLimits);
    if (approvalLimitCheck?.exceeded) {
      emit({ type: "step_end" });
      return {
        status: "completed",
        appendedMessages,
        transcriptDelta: buildTranscriptDelta(appendedMessages),
        usage: tokenUsage,
        stopReason: approvalLimitCheck.stopReason as "max_steps" | "abort" | "error",
        engineState: continuationState,
      };
    }

    counters.approvalRequests++;
    const pending = createPendingApproval(
      {
        requestId: toolResult.approvalRequestId,
        kind: toolResult.approvalKind,
        details: toolResult.approvalDetails,
      },
      continuationState,
    );

    emit({
      type: "approval_requested",
      request: pending,
    });

    emit({ type: "step_end" });

    return {
      status: "awaiting_approval",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: tokenUsage,
      pendingApproval: pending,
      stopReason: "approval",
      engineState: continuationState,
    };
  }

  appendedMessages.push(...toolResult.messages);

  if (settings.stopConditions?.stopOnToolResult) {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: tokenUsage,
      stopReason: "tool",
      engineState: continuationState,
    };
  }

  emit({ type: "step_end" });

  return {
    status: "continue",
    appendedMessages,
    transcriptDelta: buildTranscriptDelta(appendedMessages),
    usage: tokenUsage,
    engineState: continuationState,
  };
}
