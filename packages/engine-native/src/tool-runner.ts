import type {
  AssistantMessage,
  EngineEvent,
  EngineRunSettings,
  EngineTool,
  Message,
} from "piko-engine-protocol";
import { buildToolResultMessage } from "./transcript-builder.js";
import type { NativeToolRegistry } from "./types.js";

interface PendingToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  executorTarget?: string;
  executionMode?: "sequential" | "parallel";
}

/** Serialisable snapshot of pending tool calls stored in engine state. */
export interface PendingToolSnapshot {
  /** Remaining tool calls that need execution (index 0 = the one needing approval). */
  remainingToolCalls: PendingToolCall[];
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
 * Approval is preflighted in original call order. When a tool requires approval,
 * only the calls before it are executed and the approval-gated call plus the rest
 * are returned so the state machine can resume them after approval.
 */
export async function executeToolCalls(
  assistantMessage: AssistantMessage,
  tools: EngineTool[],
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  settings?: Pick<EngineRunSettings, "parallelTools">,
  signal?: AbortSignal,
  /** If set, skip tools before this call ID (used after approval resolution). */
  startAfterCallId?: string,
): Promise<ToolExecutionResult> {
  let toolCalls = assistantMessage.content.filter((c) => c.type === "toolCall");

  if (toolCalls.length === 0) {
    return { messages: [], approvalNeeded: false };
  }

  if (startAfterCallId) {
    const startIndex = toolCalls.findIndex((tc) => tc.id === startAfterCallId);
    toolCalls = startIndex === -1 ? [] : toolCalls.slice(startIndex);
  }

  const toolByName = new Map(tools.map((tool) => [tool.name, tool]));
  const approvalIndex = toolCalls.findIndex((tc) => {
    const toolDef = toolByName.get(tc.name);
    return Boolean(toolDef?.metadata?.requiresApproval);
  });

  const executableToolCalls = approvalIndex === -1 ? toolCalls : toolCalls.slice(0, approvalIndex);
  const messages = await executeToolCallBatch(
    executableToolCalls,
    toolByName,
    registry,
    emit,
    settings,
    signal,
  );

  if (approvalIndex !== -1) {
    const tc = toolCalls[approvalIndex];
    const remainingToolCalls = toolCalls.slice(approvalIndex).map((c) => {
      const remainingToolDef = toolByName.get(c.name);
      return {
        id: c.id,
        name: c.name,
        arguments: c.arguments,
        executorTarget: remainingToolDef?.executor.target,
        executionMode: remainingToolDef?.executionMode,
      };
    });
    return {
      messages,
      approvalNeeded: true,
      approvalRequestId: tc.id,
      approvalKind: `tool:${tc.name}`,
      approvalDetails: {
        toolName: tc.name,
        toolCallId: tc.id,
        arguments: tc.arguments,
      },
      pendingToolSnapshot: { remainingToolCalls },
    };
  }

  return {
    messages,
    approvalNeeded: false,
  };
}

export async function executePendingToolCalls(
  pendingToolCalls: PendingToolSnapshot["remainingToolCalls"],
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  settings?: Pick<EngineRunSettings, "parallelTools">,
  signal?: AbortSignal,
): Promise<Message[]> {
  const toolCalls = pendingToolCalls.map((pending) => ({
    type: "toolCall" as const,
    id: pending.id,
    name: pending.name,
    arguments: pending.arguments,
  }));
  const toolByName = new Map(
    pendingToolCalls.map((pending) => [
      pending.name,
      {
        name: pending.name,
        description: `Tool: ${pending.name}`,
        inputSchema: { type: "object", properties: {} },
        executor: { kind: "native" as const, target: pending.executorTarget ?? pending.name },
        executionMode: pending.executionMode,
      } satisfies EngineTool,
    ]),
  );
  return executeToolCallBatch(toolCalls, toolByName, registry, emit, settings, signal);
}

async function executeToolCallBatch(
  toolCalls: Array<{
    id: string;
    name: string;
    arguments: Record<string, unknown>;
  }>,
  toolByName: Map<string, EngineTool>,
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  settings?: Pick<EngineRunSettings, "parallelTools">,
  signal?: AbortSignal,
): Promise<Message[]> {
  if (toolCalls.length === 0 || signal?.aborted) return [];

  const forceSequential =
    settings?.parallelTools === false ||
    toolCalls.some((tc) => toolByName.get(tc.name)?.executionMode === "sequential");

  if (forceSequential) {
    const messages: Message[] = [];
    for (const tc of toolCalls) {
      if (signal?.aborted) break;
      messages.push(await executeSingleToolCall(tc, toolByName.get(tc.name), registry, emit));
    }
    return messages;
  }

  const settledMessages = await Promise.all(
    toolCalls.map((tc) => executeSingleToolCall(tc, toolByName.get(tc.name), registry, emit)),
  );

  return settledMessages;
}

async function executeSingleToolCall(
  tc: { id: string; name: string; arguments: Record<string, unknown> },
  toolDef: EngineTool | undefined,
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
): Promise<Message> {
  if (!toolDef) {
    const errorText = `Tool not found: ${tc.name}`;
    emit({
      type: "tool_call_end",
      id: tc.id,
      result: errorText,
      isError: true,
    });
    return buildToolResultMessage(tc.id, tc.name, errorText, true);
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
    emit({
      type: "tool_call_end",
      id: tc.id,
      result,
      isError: false,
    });
    return buildToolResultMessage(tc.id, tc.name, result, false);
  } catch (err) {
    const errorText = err instanceof Error ? err.message : String(err);
    emit({
      type: "tool_call_end",
      id: tc.id,
      result: errorText,
      isError: true,
    });
    return buildToolResultMessage(tc.id, tc.name, errorText, true);
  }
}
