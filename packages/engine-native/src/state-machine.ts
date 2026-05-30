import type { Message } from "@earendil-works/pi-ai";
import type {
  EngineInput,
  EngineEvent,
  EngineStepResult,
  EngineApprovalResolution,
  TokenUsage,
} from "piko-engine-protocol";
import type { NativeToolRegistry } from "./types.ts";
import { runProviderCall } from "./provider-runner.js";
import { executeToolCalls } from "./tool-runner.js";
import { createPendingApproval, validateApprovalResolution } from "./approval-state.js";
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
  const assistantMessage = providerResult.assistantMessage;
  const tokenUsage = providerResult.tokenUsage;

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
  const toolCalls = assistantMessage.content.filter(
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
    // Build a tool result message indicating the user declined
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

  // Accept or acceptForSession: tool was already executed inline or needs re-execution
  // For accepted tools, we would need to re-execute. But the tool call was already
  // intercepted before execution. We need to look up which tool to execute.
  // For now, we return a placeholder. In a real implementation, the approval
  // state would carry enough context to re-execute the tool.
  emit({ type: "step_end" });

  return {
    status: "completed",
    appendedMessages,
    stopReason: "approval",
    engineState: resolution.engineState,
  };
}
