// ---- Model step state machine — pi-ai call → prepare tool calls (no execution) ----

import type { AssistantMessage, Model } from "@earendil-works/pi-ai";
import { stream as piStream } from "@earendil-works/pi-ai";
import { buildContinuationState, getOrCreateCounters } from "./continuation-state.js";
import type { Usage } from "./event-stream.js";
import { prepareToolCalls } from "./tool-runner.js";
import type { ModelStepEvent, ModelStepInput, ModelStepResult } from "./types.js";

/**
 * Run one step: pi-ai call → prepare tool calls (no execution).
 *
 * Pure model-level logic. Does not execute tools or request approval.
 * Returns `awaiting_resource` when tool calls are detected; the caller
 * (AgentActor + ToolActor) handles execution and resolution.
 */
export async function runModelStepStateMachine(
  input: ModelStepInput,
  emit: (event: ModelStepEvent) => void,
  signal?: AbortSignal,
): Promise<ModelStepResult> {
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

  emit({ type: "step_start" });
  counters.modelCalls++;

  // ---- Phase 1: pi-ai call ----
  const callResult = await callPiAi(input, emit, signal);
  const assistantMessage = callResult.assistantMessage;

  if (callResult.isError || assistantMessage.role !== "assistant") {
    counters.consecutiveErrors++;
    emit({ type: "step_end" });
    return {
      status: "error",
      appendedMessages: [assistantMessage],
      transcriptDelta: [{ kind: "assistant_message", message: assistantMessage }],
      stopReason: "error",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  const appendedMessages = [assistantMessage];

  // Check stop conditions
  if (settings.stopConditions?.stopOnAssistantMessage) {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: appendedMessages.map(
        (m) => ({ kind: "assistant_message", message: m }) as const,
      ),
      usage: callResult.tokenUsage,
      stopReason: "assistant",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  // ---- Phase 2: Prepare tool calls (no execution) ----
  const toolCalls = Array.isArray(assistantMessage.content)
    ? assistantMessage.content.filter((c: unknown) => (c as { type?: string }).type === "toolCall")
    : [];

  if (!settings.allowToolCalls || toolCalls.length === 0) {
    emit({ type: "step_end" });
    return {
      status: "completed",
      appendedMessages,
      transcriptDelta: appendedMessages.map(
        (m) => ({ kind: "assistant_message", message: m }) as const,
      ),
      usage: callResult.tokenUsage,
      stopReason: "assistant",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  const toolPrep = prepareToolCalls(assistantMessage, effectiveTools);

  if (toolPrep.kind === "completed") {
    emit({ type: "step_end" });
    const messages = [...appendedMessages, ...toolPrep.messages];
    return {
      status: "completed",
      appendedMessages: messages,
      transcriptDelta: messages.map((m) => ({ kind: "assistant_message", message: m }) as const),
      stopReason: "tool",
      engineState: buildContinuationState(input, assistantMessage, counters),
    };
  }

  // ---- Phase 3: Await resource resolution ----
  const continuationState = buildContinuationState(input, assistantMessage, counters, {
    pendingToolSnapshot: toolPrep.pendingToolSnapshot,
  });

  emit({
    type: "resource_requested",
    request: {
      assistantMessage,
      remainingToolCallIds: toolPrep.pendingToolSnapshot!.remainingToolCalls.map((tc) => tc.id),
      toolCalls: toolPrep.pendingToolSnapshot!.remainingToolCalls.map((tc) => ({
        id: tc.id,
        name: tc.name,
        args: tc.arguments,
        executorTarget: tc.executorTarget,
        executionMode: tc.executionMode,
      })),
      settings,
    },
  });

  emit({ type: "step_end" });

  const messages = [...appendedMessages, ...toolPrep.messages];
  return {
    status: "awaiting_resource",
    appendedMessages: messages,
    transcriptDelta: messages.map((m) => ({ kind: "assistant_message", message: m }) as const),
    usage: callResult.tokenUsage,
    pendingTools: continuationState.pendingToolCalls!,
    stopReason: "resource",
    engineState: continuationState,
  };
}

// ---- pi-ai call (inline, no adapter layer) ----

interface PiCallResult {
  assistantMessage: AssistantMessage;
  tokenUsage: Usage;
  isError: boolean;
}

const emptyUsage: Usage = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};

function buildErrorAssistantMessage(text: string): AssistantMessage {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    api: "openai-completions",
    provider: "unknown",
    model: "unknown",
    usage: emptyUsage,
    stopReason: "error",
    timestamp: Date.now(),
  };
}

async function callPiAi(
  input: ModelStepInput,
  emit: (event: ModelStepEvent) => void,
  signal?: AbortSignal,
): Promise<PiCallResult> {
  const { model, provider, transcript, systemPrompt, tools, settings } = input;

  try {
    const s = piStream(
      model as Model<string>,
      {
        systemPrompt,
        messages: transcript,
        tools: tools?.length
          ? tools.map((t) => ({
              name: t.name,
              description: t.description,
              parameters: t.inputSchema as never,
            }))
          : undefined,
      },
      {
        apiKey: provider.apiKey,
        headers: provider.headers,
        baseUrl: provider.baseUrl,
        reasoning:
          settings.thinkingLevel && settings.thinkingLevel !== "off"
            ? settings.thinkingLevel
            : undefined,
        signal,
      },
    );

    let assistantMessage: AssistantMessage | undefined;
    let streamError = false;

    for await (const event of s) {
      switch (event.type) {
        case "text_delta":
          emit({
            type: "message_delta",
            messageId: "assistant",
            delta: event.delta,
          });
          break;
        case "thinking_delta":
          emit({
            type: "thinking_delta",
            messageId: "assistant",
            delta: event.delta,
          });
          break;
        case "toolcall_start": {
          const tc = event.partial.content[event.contentIndex];
          if (tc?.type === "toolCall") {
            emit({
              type: "provider_tool_call_delta",
              id: tc.id,
              name: tc.name,
              argsDelta: undefined,
            });
          }
          break;
        }
        case "done":
          assistantMessage = event.message;
          break;
        case "error":
          streamError = true;
          assistantMessage = event.error;
          break;
      }
    }

    if (!assistantMessage) {
      const err = buildErrorAssistantMessage("No response from provider");
      return { assistantMessage: err, tokenUsage: emptyUsage, isError: true };
    }

    const usage: Usage = assistantMessage.usage
      ? {
          input: assistantMessage.usage.input,
          output: assistantMessage.usage.output,
          cacheRead: assistantMessage.usage.cacheRead,
          cacheWrite: assistantMessage.usage.cacheWrite,
          totalTokens: assistantMessage.usage.totalTokens,
          cost: {
            input: assistantMessage.usage.cost.input,
            output: assistantMessage.usage.cost.output,
            cacheRead: assistantMessage.usage.cost.cacheRead,
            cacheWrite: assistantMessage.usage.cost.cacheWrite,
            total: assistantMessage.usage.cost.total,
          },
        }
      : emptyUsage;

    emit({ type: "message_end", message: assistantMessage });

    return { assistantMessage, tokenUsage: usage, isError: streamError };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const errMsg = buildErrorAssistantMessage(message);
    return { assistantMessage: errMsg, tokenUsage: emptyUsage, isError: true };
  }
}
