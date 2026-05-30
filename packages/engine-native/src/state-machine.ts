import type {
  Message,
  EngineInput,
  EngineEvent,
  EngineStepResult,
  EngineApprovalResolution,
} from "piko-engine-protocol";
import type { NativeToolRegistry } from "./types.js";
import { runProviderCall } from "./provider-runner.js";
import { executeToolCalls } from "./tool-runner.js";
import { createPendingApproval } from "./approval-state.js";
import { buildToolResultMessage } from "./transcript-builder.js";

export async function runStepStateMachine(
  input: EngineInput,
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
): Promise<EngineStepResult> {
  const { settings, tools } = input;

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
  const toolCalls = contentBlocks.filter(
    (c) => c.type === "toolCall",
  );

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
    tools,
    registry,
    emit,
    signal,
  );

  // Check for approval
  if (toolResult.approvalNeeded) {
    const pending = createPendingApproval(
      {
        requestId: toolResult.approvalRequestId!,
        kind: toolResult.approvalKind!,
        details: toolResult.approvalDetails,
      },
      input.engineState,
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
      engineState: input.engineState,
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

  emit({ type: "step_end" });

  return {
    status: "completed",
    appendedMessages,
    stopReason: "approval",
    engineState: resolution.engineState,
  };
}
