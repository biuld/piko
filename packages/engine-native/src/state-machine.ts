import type {
  EngineApprovalResolution,
  EngineContinuationState,
  EngineEvent,
  EngineInput,
  EngineRunSettings,
  EngineStepResult,
  Message,
  TranscriptDelta,
} from "piko-engine-protocol";
import { createPendingApproval, extractContinuationState } from "./approval-state.js";
import { piAiAdapter as defaultAdapter } from "./provider/pi-ai-adapter.js";
import type { ProviderAdapter } from "./provider/types.js";
import { runProviderCall } from "./provider-runner.js";
import {
  checkBeforeApproval,
  checkBeforeModelCall,
  checkBeforeToolCall,
  createCounters,
} from "./runtime-limits.js";
import { executePendingToolCalls, executeToolCalls } from "./tool-runner.js";
import { buildToolResultMessage } from "./transcript-builder.js";
import type { NativeToolRegistry } from "./types.js";

/** Build transcript deltas from the messages appended in a step. */
function buildTranscriptDelta(messages: Message[]): TranscriptDelta[] {
  const deltas: TranscriptDelta[] = [];
  for (const msg of messages) {
    if (msg.role === "assistant") {
      deltas.push({ kind: "assistant_message", message: msg });
    } else if (msg.role === "toolResult") {
      deltas.push({
        kind: "tool_result",
        message: msg,
        toolCallId: msg.toolCallId,
      });
    }
  }
  return deltas;
}

/**
 * Build a typed EngineContinuationState from a step's outcome.
 */
function buildContinuationState(
  input: EngineInput,
  assistantMessage: Message,
  counters: import("piko-engine-protocol").EngineRuntimeCounters,
  toolResult?: {
    pendingToolSnapshot?: {
      remainingToolCalls: Array<{
        id: string;
        name: string;
        arguments: Record<string, unknown>;
        executorTarget?: string;
        executionMode?: "sequential" | "parallel";
      }>;
    };
  },
): EngineContinuationState {
  if (toolResult?.pendingToolSnapshot) {
    const remaining = toolResult.pendingToolSnapshot.remainingToolCalls;
    return {
      version: 1,
      pendingToolCalls: {
        assistantMessage,
        remainingToolCallIds: remaining.map((tc) => tc.id),
        toolCalls: remaining.map((tc) => ({
          id: tc.id,
          name: tc.name,
          args: tc.arguments,
          executorTarget: tc.executorTarget,
          executionMode: tc.executionMode,
        })),
        settings: {
          parallelTools: input.settings.parallelTools,
          runtimeLimits: input.settings.runtimeLimits,
        },
      },
      counters,
    };
  }

  return {
    version: 1,
    counters,
  };
}

function extractContinuationStateFromInput(
  input: EngineInput,
): EngineContinuationState | undefined {
  const raw = input.engineState;
  if (!raw) return undefined;

  // New typed format
  if (
    typeof raw === "object" &&
    raw !== null &&
    "version" in raw &&
    (raw as EngineContinuationState).version === 1
  ) {
    return raw as EngineContinuationState;
  }

  return undefined;
}

function getOrCreateCounters(input: EngineInput) {
  const prev = extractContinuationStateFromInput(input);
  return prev?.counters ?? createCounters();
}

export async function runStepStateMachine(
  input: EngineInput,
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
  adapter?: ProviderAdapter,
): Promise<EngineStepResult> {
  const { settings, tools } = input;
  const effectiveTools = tools ?? [];

  // Check AbortSignal before starting
  if (signal?.aborted) {
    return {
      status: "aborted",
      appendedMessages: [],
      transcriptDelta: [],
      stopReason: "abort",
      engineState: input.engineState,
    };
  }

  // Initialize or carry forward runtime counters
  const counters = getOrCreateCounters(input);

  // Check runtime limits before model call
  const limitCheck = checkBeforeModelCall(counters, settings.runtimeLimits);
  if (limitCheck?.exceeded) {
    return {
      status: "completed",
      appendedMessages: [],
      transcriptDelta: [],
      stopReason: limitCheck.stopReason as "max_steps" | "abort" | "error",
      engineState: { version: 1, counters },
    };
  }

  // Step 1: Make the provider call
  emit({ type: "step_start" });
  counters.modelCalls++;

  const providerResult = await runProviderCall(input, emit, signal, adapter ?? defaultAdapter);
  const resultMessage = providerResult.assistantMessage;
  const tokenUsage = providerResult.tokenUsage;

  // Provider error: return error status so Host can retry or fail
  if (providerResult.isError) {
    counters.consecutiveErrors++;
    emit({ type: "step_end" });
    return {
      status: "error",
      appendedMessages: [resultMessage],
      transcriptDelta: buildTranscriptDelta([resultMessage]),
      stopReason: "error",
      engineState: { version: 1, counters },
    };
  }

  // Narrow: the provider always returns an AssistantMessage
  if (resultMessage.role !== "assistant") {
    counters.consecutiveErrors++;
    emit({ type: "step_end" });
    return {
      status: "error",
      appendedMessages: [resultMessage],
      transcriptDelta: buildTranscriptDelta([resultMessage]),
      stopReason: "error",
      engineState: { version: 1, counters },
    };
  }

  const assistantMessage = resultMessage;
  const appendedMessages: Message[] = [assistantMessage];

  // Check for stop conditions
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

  // Check if assistant message contains tool calls
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

  // Step 2: Execute tool calls (with runtime limit enforcement)
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
    emit, // emit both render and tool lifecycle events through EngineEvent stream
    settings,
    signal,
    undefined, // startAfterCallId
    counters, // per-tool limit enforcement increments this
  );

  const continuationState = buildContinuationState(input, assistantMessage, counters, toolResult);

  if (toolResult.limitReached) {
    for (const msg of toolResult.messages) {
      appendedMessages.push(msg);
    }
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: buildTranscriptDelta(appendedMessages),
      usage: tokenUsage,
      stopReason: toolResult.limitStopReason ?? "max_steps",
      engineState: continuationState,
    };
  }

  // Check for approval
  if (toolResult.approvalNeeded) {
    // Check approval limit
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
        requestId: toolResult.approvalRequestId!,
        kind: toolResult.approvalKind!,
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

  // Add tool results to appended messages
  for (const msg of toolResult.messages) {
    appendedMessages.push(msg);
  }

  // Check stop on tool result
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

export async function runApprovalResolution(
  resolution: EngineApprovalResolution,
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
): Promise<EngineStepResult> {
  const { decision } = resolution;

  const appendedMessages: Message[] = [];
  const continuationState = extractContinuationState(resolution);

  if (decision === "decline") {
    // Decline produces a durable tool-result denial message
    const declineMsg = buildToolResultMessage(
      resolution.approvalRequestId,
      "approval",
      "User declined the tool execution",
      false,
    );
    appendedMessages.push(declineMsg);

    emit({ type: "step_end" });

    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: [
        ...buildTranscriptDelta(appendedMessages),
        {
          kind: "approval_record" as const,
          requestId: resolution.approvalRequestId,
          decision: "decline" as const,
        },
      ],
      stopReason: "approval",
      engineState: continuationState,
    };
  }

  // Accept / acceptForSession: resume execution from the pending tool call state
  // acceptForSession: engine does NOT store cross-step permission memory;
  // Host owns session-level policy.
  const pendingToolCalls = continuationState?.pendingToolCalls;

  if (pendingToolCalls && pendingToolCalls.remainingToolCallIds.length > 0) {
    const resumeSettings: Pick<EngineRunSettings, "parallelTools" | "runtimeLimits"> = {
      parallelTools: pendingToolCalls.settings?.parallelTools,
      runtimeLimits: pendingToolCalls.settings?.runtimeLimits,
    };
    const toolMessages = await executePendingToolCalls(
      pendingToolCalls.toolCalls.map((tc) => ({
        id: tc.id,
        name: tc.name,
        arguments: tc.args,
        executorTarget: tc.executorTarget,
        executionMode: tc.executionMode,
      })),
      registry,
      emit,
      resumeSettings,
      signal,
      continuationState?.counters,
    );
    appendedMessages.push(...toolMessages);
  }

  emit({ type: "step_end" });

  // After approval resolution, clear pendingToolCalls but keep counters
  const nextContinuation: EngineContinuationState = {
    version: 1,
    pendingToolCalls: undefined,
    counters: continuationState?.counters ?? createCounters(),
  };

  return {
    status: "continue",
    appendedMessages,
    transcriptDelta: [
      ...buildTranscriptDelta(appendedMessages),
      {
        kind: "approval_record" as const,
        requestId: resolution.approvalRequestId,
        decision: resolution.decision,
      },
    ],
    stopReason: "approval",
    engineState: nextContinuation,
  };
}
