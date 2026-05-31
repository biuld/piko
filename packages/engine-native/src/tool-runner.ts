import type { AssistantMessage, EngineEvent, EngineTool, Message } from "piko-engine-protocol";
import { buildToolResultMessage } from "./transcript-builder.js";
import type { NativeToolRegistry } from "./types.js";

/** Serialisable snapshot of pending tool calls stored in engine state. */
export interface PendingToolSnapshot {
  /** Remaining tool calls that need execution (index 0 = the one needing approval). */
  remainingToolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>;
}

export interface ToolExecutionResult {
  messages: Message[];
  approvalNeeded: boolean;
  approvalRequestId?: string;
  approvalKind?: string;
  approvalDetails?: unknown;
  /** Snapshot of pending tool calls for the engine state (only set when approvalNeeded). */
  pendingToolSnapshot?: PendingToolSnapshot;
}

/**
 * Execute tool calls from an assistant message, with optional approval gating.
 *
 * Iterates through tool calls sequentially. When a tool requires approval,
 * execution stops and a snapshot of remaining (unexecuted) tool calls is
 * returned so the state machine can resume them after approval.
 */
export async function executeToolCalls(
  assistantMessage: AssistantMessage,
  tools: EngineTool[],
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
  /** If set, skip tools before this call ID (used after approval resolution). */
  startAfterCallId?: string,
): Promise<ToolExecutionResult> {
  const toolCalls = assistantMessage.content.filter((c) => c.type === "toolCall");

  if (toolCalls.length === 0) {
    return { messages: [], approvalNeeded: false };
  }

  const messages: Message[] = [];
  let approvalNeeded = false;
  let approvalRequestId: string | undefined;
  let approvalKind: string | undefined;
  let approvalDetails: unknown;
  let started = !startAfterCallId; // If no startAfterCallId, start immediately

  for (let i = 0; i < toolCalls.length; i++) {
    const tc = toolCalls[i];

    // Skip tool calls before the start point (for resumed execution)
    if (!started) {
      if (tc.id === startAfterCallId) {
        started = true;
        // Fall through to execute this tool call
      } else {
        continue;
      }
    }

    if (signal?.aborted) break;

    const toolDef = tools.find((t) => t.name === tc.name);
    if (!toolDef) {
      const errorMsg = buildToolResultMessage(tc.id, tc.name, `Tool not found: ${tc.name}`, true);
      messages.push(errorMsg);
      emit({
        type: "tool_call_end",
        id: tc.id,
        result: `Tool not found: ${tc.name}`,
        isError: true,
      });
      continue;
    }

    // Check for approval requirement
    if (toolDef.metadata?.requiresApproval) {
      approvalNeeded = true;
      approvalRequestId = tc.id;
      approvalKind = `tool:${tc.name}`;
      approvalDetails = {
        toolName: tc.name,
        toolCallId: tc.id,
        arguments: tc.arguments,
      };
      // Capture remaining tool calls (including this one) for engine state
      const remainingToolCalls = toolCalls.slice(i).map((c) => ({
        id: c.id,
        name: c.name,
        arguments: c.arguments,
      }));
      return {
        messages,
        approvalNeeded: true,
        approvalRequestId,
        approvalKind,
        approvalDetails,
        pendingToolSnapshot: { remainingToolCalls },
      };
    }

    emit({
      type: "tool_call_start",
      id: tc.id,
      name: tc.name,
      args: tc.arguments,
    });

    try {
      const executor = registry[toolDef.executor.target];
      if (!executor) {
        throw new Error(`No executor registered for target: ${toolDef.executor.target}`);
      }

      const result = await executor(tc.arguments);
      const msg = buildToolResultMessage(tc.id, tc.name, result, false);
      messages.push(msg);

      emit({
        type: "tool_call_end",
        id: tc.id,
        result,
        isError: false,
      });
    } catch (err) {
      const errorText = err instanceof Error ? err.message : String(err);
      const msg = buildToolResultMessage(tc.id, tc.name, errorText, true);
      messages.push(msg);

      emit({
        type: "tool_call_end",
        id: tc.id,
        result: errorText,
        isError: true,
      });
    }
  }

  return {
    messages,
    approvalNeeded: false,
  };
}
