import type { AssistantMessage, Model } from "@earendil-works/pi-ai";
import { stream as piStream } from "@earendil-works/pi-ai";
import type { EngineEvent, EngineInput, TokenUsage } from "piko-engine-protocol";
import { buildErrorMessage } from "./transcript-builder.js";

export interface ProviderResult {
  assistantMessage: AssistantMessage;
  tokenUsage: TokenUsage;
}

function createEmptyTokenUsage(): TokenUsage {
  return {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    total: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
  };
}

function toPiModel(model: EngineInput["model"]): Model<string> {
  return {
    id: model.id,
    name: model.name,
    api: model.api as import("@earendil-works/pi-ai").Api,
    provider: model.provider,
    baseUrl: model.baseUrl,
    reasoning: model.reasoning,
    input: model.input as ("text" | "image")[],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: model.contextWindow,
    maxTokens: model.maxTokens,
  };
}

function fromPiUsage(usage: AssistantMessage["usage"]): TokenUsage {
  return {
    input: usage.input,
    output: usage.output,
    cacheRead: usage.cacheRead,
    cacheWrite: usage.cacheWrite,
    totalTokens: usage.totalTokens,
    total: usage.totalTokens,
    cost: {
      input: usage.cost.input,
      output: usage.cost.output,
      cacheRead: usage.cost.cacheRead,
      cacheWrite: usage.cost.cacheWrite,
      total: usage.cost.total,
    },
  };
}

export async function runProviderCall(
  input: EngineInput,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
): Promise<ProviderResult> {
  const { model, provider, transcript, systemPrompt, tools } = input;

  emit({ type: "step_start" });

  const piModel = toPiModel(model);

  const piTools =
    tools.length > 0
      ? tools.map((t) => ({
          name: t.name,
          description: t.description,
          parameters: t.inputSchema as never,
        }))
      : undefined;

  const context = {
    systemPrompt,
    messages: transcript,
    tools: piTools,
  };

  const providerOptions: Record<string, unknown> = {};
  if (provider.apiKey) providerOptions.apiKey = provider.apiKey;
  if (provider.headers) providerOptions.headers = provider.headers;
  if (signal) providerOptions.signal = signal;
  if (provider.reasoning?.effort) providerOptions.reasoning = provider.reasoning.effort;

  try {
    const s = piStream(piModel, context, providerOptions);
    let piAssistantMessage: AssistantMessage | undefined;

    for await (const event of s) {
      if (event.type === "text_delta") {
        emit({ type: "message_delta", messageId: "assistant", delta: event.delta });
      } else if (event.type === "thinking_delta") {
        emit({ type: "thinking_delta", messageId: "assistant", delta: event.delta });
      } else if (event.type === "toolcall_start") {
        const tc = event.partial.content[event.contentIndex];
        if (tc?.type === "toolCall") {
          emit({ type: "tool_call_start", id: tc.id, name: tc.name, args: tc.arguments });
        }
      } else if (event.type === "done") {
        piAssistantMessage = event.message;
      } else if (event.type === "error") {
        piAssistantMessage = event.error;
      }
    }

    if (!piAssistantMessage) {
      return {
        assistantMessage: buildErrorMessage("No response from provider"),
        tokenUsage: createEmptyTokenUsage(),
      };
    }

    emit({ type: "message_end", message: piAssistantMessage });

    return {
      assistantMessage: piAssistantMessage,
      tokenUsage: fromPiUsage(piAssistantMessage.usage),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      assistantMessage: buildErrorMessage(message),
      tokenUsage: createEmptyTokenUsage(),
    };
  }
}
