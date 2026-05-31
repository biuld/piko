import type {
  EngineApprovalResolution,
  EngineEvent,
  EngineInput,
  EngineStepResult,
  Message,
} from "piko-engine-protocol";
import { createPendingApproval } from "./approval-state.js";
import { runProviderCall } from "./provider-runner.js";
import {
  executePendingToolCalls,
  executeToolCalls,
  type PendingToolSnapshot,
} from "./tool-runner.js";
import { buildToolResultMessage } from "./transcript-builder.js";
import type { NativeToolRegistry } from "./types.js";

export async function runStepStateMachine(
  input: EngineInput,
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
): Promise<EngineStepResult> {
  const { settings, tools } = input;
  const effectiveTools = tools ?? [];

  // Step 1: Make the provider call
  emit({ type: "step_start" });

  const providerResult = await runProviderCall(input, emit, signal);
  const resultMessage = providerResult.assistantMessage;
  const tokenUsage = providerResult.tokenUsage;

  // Narrow: the provider always returns an AssistantMessage
  if (resultMessage.role !== "assistant") {
    emit({ type: "step_end" });
    return {
      status: "error",
      appendedMessages: [resultMessage],
      stopReason: "error",
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
      usage: tokenUsage,
      stopReason: "assistant",
      engineState: input.engineState,
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
      usage: tokenUsage,
      stopReason: "assistant",
      engineState: input.engineState,
    };
  }

  // Step 2: Execute tool calls
  const toolResult = await executeToolCalls(
    assistantMessage,
    effectiveTools,
    registry,
    emit,
    settings,
    signal,
  );

  // Check for approval
  if (toolResult.approvalNeeded) {
    // Store pending tool snapshot in engine state so it can be resumed after approval
    const approvalEngineState = {
      ...((input.engineState as Record<string, unknown>) ?? {}),
      pendingToolSnapshot: toolResult.pendingToolSnapshot,
      pendingToolSettings: { parallelTools: settings.parallelTools },
    };

    const pending = createPendingApproval(
      {
        requestId: toolResult.approvalRequestId!,
        kind: toolResult.approvalKind!,
        details: toolResult.approvalDetails,
      },
      approvalEngineState,
    );

    emit({
      type: "approval_requested",
      request: pending,
    });

    emit({ type: "step_end" });

    return {
      status: "awaiting_approval",
      appendedMessages,
      usage: tokenUsage,
      pendingApproval: pending,
      stopReason: "approval",
      engineState: approvalEngineState,
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
      usage: tokenUsage,
      stopReason: "tool",
      engineState: input.engineState,
    };
  }

  emit({ type: "step_end" });

  return {
    status: "continue",
    appendedMessages,
    usage: tokenUsage,
    engineState: input.engineState,
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

  if (decision === "decline") {
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
      stopReason: "approval",
      engineState: resolution.engineState,
    };
  }

  // Accept: resume execution from the pending tool call snapshot
  const engineState = resolution.engineState as Record<string, unknown> | undefined;
  const pendingSnapshot = engineState?.pendingToolSnapshot as PendingToolSnapshot | undefined;
  const pendingToolSettings = engineState?.pendingToolSettings as
    | { parallelTools?: boolean }
    | undefined;

  if (pendingSnapshot && pendingSnapshot.remainingToolCalls.length > 0) {
    const toolMessages = await executePendingToolCalls(
      pendingSnapshot.remainingToolCalls,
      registry,
      emit,
      pendingToolSettings,
      signal,
    );
    appendedMessages.push(...toolMessages);
  }

  emit({ type: "step_end" });

  return {
    status: "continue",
    appendedMessages,
    stopReason: "approval",
    engineState: resolution.engineState,
  };
}
