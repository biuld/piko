import type { AssistantMessage, Message, ToolDef } from "piko-protocol";

// ---- Pending tool calls (serialisable snapshot) ----

export interface PendingToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  executorTarget?: string;
  executionMode?: "sequential" | "parallel";
}

export interface PendingToolSnapshot {
  remainingToolCalls: PendingToolCall[];
}

// ---- Result ----

export type ToolExecutionResult =
  | { kind: "completed"; messages: Message[] }
  | { kind: "awaiting_resource"; messages: Message[]; pendingToolSnapshot: PendingToolSnapshot }
  | {
      kind: "limit_reached";
      messages: Message[];
      limitStopReason: "max_steps" | "abort" | "error";
    };

/**
 * Prepare tool calls from an assistant message.
 *
 * Engine does NOT execute tools. It validates existence and returns
 * a snapshot of pending calls. The caller (orchestrator) is responsible
 * for execution and calling engine.resolveResource() to resume.
 */
export function prepareToolCalls(
  assistantMessage: AssistantMessage,
  tools: ToolDef[],
): Pick<ToolExecutionResult, "kind"> & {
  pendingToolSnapshot?: PendingToolSnapshot;
  messages: Message[];
} {
  const toolCalls = assistantMessage.content.filter((c) => c.type === "toolCall");
  if (toolCalls.length === 0) {
    return { kind: "completed", messages: [] };
  }

  const toolByName = new Map(tools.map((t) => [t.name, t]));
  const pending: PendingToolCall[] = [];

  for (const tc of toolCalls) {
    const toolDef = toolByName.get(tc.name);
    pending.push({
      id: tc.id,
      name: tc.name,
      arguments: tc.arguments,
      executorTarget: toolDef?.executor.target,
      executionMode: toolDef?.executionMode,
    });
  }

  return {
    kind: "awaiting_resource",
    pendingToolSnapshot: { remainingToolCalls: pending },
    messages: [],
  };
}

/**
 * Execute tool calls that were previously pending (after resource resolution).
 * Used by engine.resolveResource() to apply resolved tool results.
 */
export function executePendingToolCalls(
  pendingToolCalls: PendingToolCall[],
  results: Array<{ toolCallId: string; result: unknown; isError: boolean }>,
): Message[] {
  const messages: Message[] = [];
  const resultById = new Map(results.map((r) => [r.toolCallId, r]));

  for (const pending of pendingToolCalls) {
    const result = resultById.get(pending.id);
    if (result) {
      messages.push({
        role: "toolResult",
        toolName: pending.name,
        toolCallId: pending.id,
        details: result.result,
        isError: result.isError,
      } as Message);
    } else {
      messages.push({
        role: "toolResult",
        toolName: pending.name,
        toolCallId: pending.id,
        details: `Tool not executed: ${pending.name}`,
        isError: true,
      } as Message);
    }
  }

  return messages;
}
