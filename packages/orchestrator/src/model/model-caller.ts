// ---- ModelStepExecutor factory — in-process pi-ai model caller ----

import type { AssistantMessage, Model } from "@earendil-works/pi-ai";
import { stream as piStream } from "@earendil-works/pi-ai";
import type { ModelCapabilities, ModelRuntimeCounters, ToolDef } from "piko-orchestrator-protocol";
import { EventStream, type Usage } from "piko-orchestrator-protocol";
import type {
  ModelContinuationState,
  ModelStepEvent,
  ModelStepExecutor,
  ModelStepInput,
  ModelStepResult,
} from "./types.js";

export interface CreateModelCallerOptions {
  /** Additional tool definitions for validation (not execution). */
  toolDefinitions?: ToolDef[];
}

export function createModelCaller(options: CreateModelCallerOptions = {}): ModelStepExecutor {
  const defs = options.toolDefinitions ?? [];

  const capabilities: ModelCapabilities = {
    supportsTools: defs.length > 0,
    supportsSandbox: false,
    supportsMCP: false,
    tools: defs.map((t) => ({ name: t.name, description: t.description })),
  };

  return {
    capabilities,

    executeStep(
      input: ModelStepInput,
      signal?: AbortSignal,
    ): EventStream<ModelStepEvent, ModelStepResult> {
      const stream = new EventStream<ModelStepEvent, ModelStepResult>();

      void runStep(
        input,
        (event) => {
          if (signal?.aborted) return;
          stream.push(event);
        },
        signal,
      )
        .then((result) => stream.end(result))
        .catch((err) => {
          const errorMsg = err instanceof Error ? err.message : String(err);
          stream.push({ type: "error", message: errorMsg });
          stream.end({
            status: "error",
            appendedMessages: [],
            stopReason: "error",
          });
        });

      return stream;
    },

    async shutdown(): Promise<void> {},
  };
}

// ---- Step runner ----

async function runStep(
  input: ModelStepInput,
  emit: (event: ModelStepEvent) => void,
  signal?: AbortSignal,
): Promise<ModelStepResult> {
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

  const result = await callPiAi(input, emit, signal);
  const assistantMessage = result.assistantMessage;

  if (result.isError || assistantMessage.role !== "assistant") {
    counters.consecutiveErrors++;
    emit({ type: "step_end" });
    return {
      status: "error",
      appendedMessages: [assistantMessage],
      transcriptDelta: [{ kind: "assistant_message", message: assistantMessage }],
      stopReason: "error",
      engineState: buildContinuationState(counters),
    };
  }

  emit({ type: "step_end" });

  return {
    status: "completed",
    appendedMessages: [assistantMessage],
    transcriptDelta: [{ kind: "assistant_message", message: assistantMessage }],
    usage: result.tokenUsage,
    stopReason: "assistant",
    engineState: buildContinuationState(counters),
  };
}

// ---- pi-ai call ----

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

// ---- Continuation state helpers (inlined) ----

function getOrCreateCounters(input: ModelStepInput): ModelRuntimeCounters {
  const prev = extractContinuationState(input);
  return (
    prev?.counters ?? {
      modelCalls: 0,
      toolCalls: 0,
      consecutiveErrors: 0,
      startedAt: Date.now(),
    }
  );
}

function buildContinuationState(counters: ModelRuntimeCounters): ModelContinuationState {
  return { version: 1, kind: "ready", counters };
}

function extractContinuationState(input: ModelStepInput): ModelContinuationState | undefined {
  const raw = input.engineState;
  if (!raw) return undefined;
  if (
    typeof raw === "object" &&
    raw !== null &&
    "version" in raw &&
    (raw as ModelContinuationState).version === 1
  ) {
    return raw as ModelContinuationState;
  }
  return undefined;
}
