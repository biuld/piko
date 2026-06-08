import type {
  EngineApprovalResolution,
  EngineEvent,
  EngineRunSettings,
  EngineStepResult,
  Message,
} from "piko-engine-protocol";
import { createPendingApproval, extractContinuationState } from "../approval-state.js";
import { createCounters } from "../runtime-limits.js";
import { executePendingToolCalls } from "../tool-runner.js";
import { buildToolResultMessage } from "../transcript-builder.js";
import type { NativeToolRegistry } from "../types.js";
import {
  buildPendingToolsContinuationState,
  createReadyContinuationState,
} from "./continuation-state.js";
import { buildTranscriptDelta } from "./transcript-delta.js";

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
    // Decline produces a durable tool-result denial message.
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

  // acceptForSession policy is owned by Host. Engine only resumes this checkpoint.
  const pendingToolCalls = continuationState?.pendingToolCalls;

  if (pendingToolCalls && pendingToolCalls.remainingToolCallIds.length > 0) {
    const resumeSettings: Pick<EngineRunSettings, "parallelTools" | "runtimeLimits"> & {
      allowApprovals?: boolean;
    } = {
      parallelTools: pendingToolCalls.settings?.parallelTools,
      runtimeLimits: pendingToolCalls.settings?.runtimeLimits,
      allowApprovals: pendingToolCalls.settings?.allowApprovals,
    };
    const counters = continuationState?.counters ?? createCounters();
    const toolResult = await executePendingToolCalls(
      pendingToolCalls.toolCalls.map((tc) => ({
        id: tc.id,
        name: tc.name,
        arguments: tc.args,
        executorTarget: tc.executorTarget,
        executionMode: tc.executionMode,
        requiresApproval: tc.requiresApproval,
      })),
      registry,
      emit,
      resumeSettings,
      signal,
      counters,
      resolution.approvalRequestId,
    );
    appendedMessages.push(...toolResult.messages);

    if (toolResult.kind === "awaiting_approval") {
      const nextContinuation = buildPendingToolsContinuationState(
        pendingToolCalls.assistantMessage,
        toolResult.pendingToolSnapshot,
        pendingToolCalls.settings,
        counters,
      );
      const pending = createPendingApproval(
        {
          requestId: toolResult.approvalRequestId,
          kind: toolResult.approvalKind,
          details: toolResult.approvalDetails,
        },
        nextContinuation,
      );
      emit({ type: "approval_requested", request: pending });
      emit({ type: "step_end" });
      return {
        status: "awaiting_approval",
        appendedMessages,
        transcriptDelta: [
          ...buildTranscriptDelta(appendedMessages),
          {
            kind: "approval_record" as const,
            requestId: resolution.approvalRequestId,
            decision: resolution.decision,
          },
        ],
        pendingApproval: pending,
        stopReason: "approval",
        engineState: nextContinuation,
      };
    }
  }

  emit({ type: "step_end" });

  const nextContinuation = createReadyContinuationState(
    continuationState?.counters ?? createCounters(),
  );

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
