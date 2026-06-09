import type {
  AssistantMessage,
  EngineEvent,
  EngineRunSettings,
  EngineRuntimeCounters,
  EngineRuntimeLimits,
  EngineTool,
  EngineToolEvent,
  Message,
} from "piko-engine-protocol";
import { checkBeforeToolCall, withToolTimeout } from "./runtime-limits.js";
import { buildToolResultMessage } from "./transcript-builder.js";
import type { NativeToolRegistry } from "./types.js";

interface PendingToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  executorTarget?: string;
  executionMode?: "sequential" | "parallel";
  requiresApproval?: boolean;
}

type ToolExecutionSettings = Pick<EngineRunSettings, "parallelTools" | "runtimeLimits"> & {
  allowApprovals?: boolean;
};

/** Serialisable snapshot of pending tool calls stored in engine state. */
export interface PendingToolSnapshot {
  /** Remaining tool calls that need execution (index 0 = the one needing approval). */
  remainingToolCalls: PendingToolCall[];
}

export type ToolExecutionResult =
  | { kind: "completed"; messages: Message[] }
  | {
      kind: "awaiting_approval";
      messages: Message[];
      approvalRequestId: string;
      approvalKind: string;
      approvalDetails: unknown;
      pendingToolSnapshot: PendingToolSnapshot;
    }
  | {
      kind: "limit_reached";
      messages: Message[];
      limitStopReason: "max_steps" | "abort" | "error";
    };

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
  emitTool: ((event: EngineToolEvent) => void) | undefined,
  settings?: ToolExecutionSettings,
  signal?: AbortSignal,
  /** If set, skip tools before this call ID (used after approval resolution). */
  startAfterCallId?: string,
  /** Mutable counters for per-tool limit enforcement. */
  counters?: EngineRuntimeCounters,
  /** Tool call already approved by the user. */
  approvedToolCallId?: string,
): Promise<ToolExecutionResult> {
  const te = emitTool ?? (() => {});
  let toolCalls = assistantMessage.content.filter((c) => c.type === "toolCall");

  if (toolCalls.length === 0) {
    return { kind: "completed", messages: [] };
  }

  if (startAfterCallId) {
    const startIndex = toolCalls.findIndex((tc) => tc.id === startAfterCallId);
    toolCalls = startIndex === -1 ? [] : toolCalls.slice(startIndex);
  }

  const toolByName = new Map(tools.map((tool) => [tool.name, tool]));

  // Phase 1: Validate all tool calls
  const validatedCalls: Array<{
    tc: (typeof toolCalls)[0];
    ok: boolean;
    error?: string;
  }> = [];
  for (const tc of toolCalls) {
    const toolDef = toolByName.get(tc.name);
    te({ type: "tool_validation_start", id: tc.id, name: tc.name });
    if (!toolDef) {
      te({
        type: "tool_validation_end",
        id: tc.id,
        ok: false,
        error: `Tool not found: ${tc.name}`,
      });
      validatedCalls.push({ tc, ok: false, error: `Tool not found: ${tc.name}` });
    } else {
      te({ type: "tool_validation_end", id: tc.id, ok: true });
      validatedCalls.push({ tc, ok: true });
    }
  }

  // Phase 2: Check approval gating (in call order)
  const approvalIndex = validatedCalls.findIndex((vc) => {
    if (!vc.ok) return false;
    if (vc.tc.id === approvedToolCallId) return false;
    const toolDef = toolByName.get(vc.tc.name);
    // Support both legacy boolean and new ToolApprovalRequirement string
    const meta = toolDef?.metadata as Record<string, unknown> | undefined;
    if (meta?.approval === "always" || meta?.approval === "on_request") return true;
    return Boolean(meta?.requiresApproval);
  });

  // Phase 3: Emit skip events for approval-required tools
  if (approvalIndex !== -1) {
    for (let i = approvalIndex; i < validatedCalls.length; i++) {
      const vc = validatedCalls[i];
      te({
        type: "tool_call_skipped",
        id: vc.tc.id,
        reason: i === approvalIndex ? "approval_required" : "approval_required",
      });
    }
  }

  // Phase 4: Execute calls before the approval-gated one (including invalid ones that return errors)
  const executableCalls =
    approvalIndex === -1 ? validatedCalls : validatedCalls.slice(0, approvalIndex);
  const batchResult = await executeToolCallBatch(
    executableCalls.map((vc) => vc.tc),
    toolByName,
    registry,
    emit,
    te,
    settings,
    signal,
    counters,
  );
  const messages = batchResult.messages;

  if (batchResult.limitReached) {
    return {
      kind: "limit_reached",
      messages,
      limitStopReason: batchResult.limitStopReason ?? "max_steps",
    };
  }

  if (approvalIndex !== -1) {
    if (settings?.allowApprovals === false) {
      const skipped = validatedCalls.slice(approvalIndex).map((v) => {
        te({
          type: "tool_call_skipped",
          id: v.tc.id,
          reason: "approval_required",
        });
        return buildToolResultMessage(
          v.tc.id,
          v.tc.name,
          `Tool skipped: ${v.tc.name} requires approval, but approvals are disabled`,
          true,
        );
      });
      return { kind: "completed", messages: [...messages, ...skipped] };
    }

    const vc = validatedCalls[approvalIndex];
    const remainingToolCalls = validatedCalls.slice(approvalIndex).map((v) => {
      const remainingToolDef = toolByName.get(v.tc.name);
      const meta = remainingToolDef?.metadata as Record<string, unknown> | undefined;
      const needsApproval =
        meta?.approval === "always" ||
        meta?.approval === "on_request" ||
        Boolean(meta?.requiresApproval);
      return {
        id: v.tc.id,
        name: v.tc.name,
        arguments: v.tc.arguments,
        executorTarget: remainingToolDef?.executor.target,
        executionMode: remainingToolDef?.executionMode,
        requiresApproval: needsApproval,
      };
    });
    return {
      kind: "awaiting_approval",
      messages,
      approvalRequestId: vc.tc.id,
      approvalKind: `tool:${vc.tc.name}`,
      approvalDetails: {
        toolName: vc.tc.name,
        toolCallId: vc.tc.id,
        arguments: vc.tc.arguments,
      },
      pendingToolSnapshot: { remainingToolCalls },
    };
  }

  return {
    kind: "completed",
    messages,
  };
}

export async function executePendingToolCalls(
  pendingToolCalls: PendingToolSnapshot["remainingToolCalls"],
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  settings?: ToolExecutionSettings,
  signal?: AbortSignal,
  counters?: EngineRuntimeCounters,
  approvedToolCallId?: string,
): Promise<ToolExecutionResult> {
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
        executor: {
          kind: "native" as const,
          target: pending.executorTarget ?? pending.name,
        },
        executionMode: pending.executionMode,
        metadata: pending.requiresApproval ? { requiresApproval: true } : undefined,
      } satisfies EngineTool,
    ]),
  );
  return executeToolCalls(
    { role: "assistant", content: toolCalls } as AssistantMessage,
    Array.from(toolByName.values()),
    registry,
    emit,
    () => {},
    settings,
    signal,
    undefined,
    counters,
    approvedToolCallId,
  );
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
  emitTool: (event: EngineToolEvent) => void,
  settings?: Pick<EngineRunSettings, "parallelTools" | "runtimeLimits">,
  signal?: AbortSignal,
  counters?: EngineRuntimeCounters,
): Promise<{
  messages: Message[];
  limitReached?: boolean;
  limitStopReason?: "max_steps" | "abort" | "error";
}> {
  if (toolCalls.length === 0 || signal?.aborted) return { messages: [] };

  const timeoutMs = settings?.runtimeLimits?.perToolTimeoutMs;
  const limits = settings?.runtimeLimits;
  const forceSequential =
    settings?.parallelTools === false ||
    toolCalls.some((tc) => toolByName.get(tc.name)?.executionMode === "sequential");

  if (forceSequential) {
    const messages: Message[] = [];
    for (const tc of toolCalls) {
      if (signal?.aborted) break;
      const result = await executeSingleToolCall(
        tc,
        toolByName.get(tc.name),
        registry,
        emit,
        emitTool,
        timeoutMs,
        signal,
        limits,
        counters,
      );
      messages.push(result.message);
      if (result.limitReached) {
        return {
          messages,
          limitReached: true,
          limitStopReason: result.limitStopReason,
        };
      }
    }
    return { messages };
  }

  // Parallel execution: preserve deterministic transcript ordering
  // by mapping results back to the original call order.
  const settledMessages = await Promise.all(
    toolCalls.map((tc, idx) =>
      executeSingleToolCall(
        tc,
        toolByName.get(tc.name),
        registry,
        emit,
        emitTool,
        timeoutMs,
        signal,
        limits,
        counters,
      ).then((result) => ({
        idx,
        result,
      })),
    ),
  );

  // Sort back to original call order
  settledMessages.sort((a, b) => a.idx - b.idx);
  const firstLimit = settledMessages.find((s) => s.result.limitReached);
  return {
    messages: settledMessages.map((s) => s.result.message),
    limitReached: Boolean(firstLimit),
    limitStopReason: firstLimit?.result.limitStopReason,
  };
}

async function executeSingleToolCall(
  tc: { id: string; name: string; arguments: Record<string, unknown> },
  toolDef: EngineTool | undefined,
  registry: NativeToolRegistry,
  emit: (event: EngineEvent) => void,
  emitTool: (event: EngineToolEvent) => void,
  perToolTimeoutMs?: number,
  signal?: AbortSignal,
  limits?: EngineRuntimeLimits,
  counters?: EngineRuntimeCounters,
): Promise<{
  message: Message;
  limitReached?: boolean;
  limitStopReason?: "max_steps" | "abort" | "error";
}> {
  if (!toolDef) {
    const errorText = `Tool not found: ${tc.name}`;
    emitTool({
      type: "tool_call_end",
      id: tc.id,
      result: errorText,
      isError: true,
    });
    return { message: buildToolResultMessage(tc.id, tc.name, errorText, true) };
  }

  // Check per-tool runtime limits before execution
  if (counters) {
    const limitCheck = checkBeforeToolCall(counters, limits);
    if (limitCheck?.exceeded) {
      emitTool({
        type: "tool_call_skipped",
        id: tc.id,
        reason: "limit",
      });
      const errMsg = buildToolResultMessage(
        tc.id,
        tc.name,
        `Tool skipped: ${limitCheck.stopReason}`,
        true,
      );
      emitTool({
        type: "tool_call_end",
        id: tc.id,
        result: errMsg,
        isError: true,
      });
      return {
        message: errMsg,
        limitReached: true,
        limitStopReason: limitCheck.stopReason as "max_steps" | "abort" | "error",
      };
    }
    counters.toolCalls++;
  }

  // Emit Engine-level tool_call_start for rendering (only through emit, not emitTool)
  emit({
    type: "tool_call_start",
    id: tc.id,
    name: tc.name,
    args: tc.arguments,
  });

  try {
    const executorFn = registry[toolDef.executor.target];
    if (!executorFn) {
      const errorText = `No executor registered for target: ${toolDef.executor.target}`;
      // Emit tool call end through emitTool only (avoiding duplicate when emit === emitTool)
      emitTool({
        type: "tool_call_end",
        id: tc.id,
        result: errorText,
        isError: true,
      });
      return { message: buildToolResultMessage(tc.id, tc.name, errorText, true) };
    }

    const result = await withToolTimeout(() => executorFn(tc.arguments), perToolTimeoutMs, signal);
    emitTool({
      type: "tool_call_end",
      id: tc.id,
      result,
      isError: false,
    });
    return { message: buildToolResultMessage(tc.id, tc.name, result, false) };
  } catch (err) {
    const errorText = err instanceof Error ? err.message : String(err);
    emitTool({
      type: "tool_call_end",
      id: tc.id,
      result: errorText,
      isError: true,
    });
    return { message: buildToolResultMessage(tc.id, tc.name, errorText, true) };
  }
}
