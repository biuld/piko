import type { Message, AssistantMessage, Context } from "@earendil-works/pi-ai";
import { stream } from "@earendil-works/pi-ai";
import type { EngineInput, EngineEvent, EngineTool, TokenUsage } from "piko-engine-protocol";
import { buildAssistantMessage, buildErrorMessage } from "./transcript-builder.js";

export interface ProviderResult {
  assistantMessage: AssistantMessage;
  tokenUsage: TokenUsage;
}

export async function runProviderCall(
  input: EngineInput,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
): Promise<ProviderResult> {
  const { model, provider, transcript, systemPrompt, tools } = input;

  // Convert EngineTool[] to pi-ai Tool[] format
  const piTools = tools.length > 0
    ? tools.map((t) => ({
        name: t.name,
        description: t.description,
        parameters: t.inputSchema as never,
      }))
    : undefined;

  const context: Context = {
    systemPrompt,
    messages: transcript,
    tools: piTools,
  };

  const providerOptions: Record<string, unknown> = {};
  if (provider.apiKey) providerOptions.apiKey = provider.apiKey;
  if (provider.headers) providerOptions.headers = provider.headers;
  if (provider.baseUrl) {
    // baseUrl override is handled via model override, not stream options
  }
  if (signal) providerOptions.signal = signal;

  // Build overrides for reasoning
  if (provider.reasoning) {
    providerOptions.reasoning = provider.reasoning.effort;
  }

  emit({ type: "step_start" });

  let assistantMessage: AssistantMessage;

  try {
    const eventStream = stream(model, context, providerOptions);

    for await (const event of eventStream) {
      if (event.type === "text_delta") {
        emit({
          type: "message_delta",
          messageId: "assistant",
          delta: event.delta,
        });
      } else if (event.type === "toolcall_start") {
        emit({
          type: "tool_call_start",
          id: event.partial.content[event.contentIndex]?.type === "toolCall"
            ? (event.partial.content[event.contentIndex] as { id: string }).id
            : "",
          name: event.partial.content[event.contentIndex]?.type === "toolCall"
            ? (event.partial.content[event.contentIndex] as { name: string }).name
            : "",
          args: {},
        });
      } else if (event.type === "done") {
        assistantMessage = event.message;
      } else if (event.type === "error") {
        assistantMessage = event.error;
      }
    }

    // If we didn't get an assistant message from the stream (unlikely), create error
    if (!assistantMessage!) {
      assistantMessage = buildErrorMessage(
        model.id,
        model.api,
        model.provider,
        "No response from provider",
      );
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    assistantMessage = buildErrorMessage(model.id, model.api, model.provider, message);
  }

  emit({
    type: "message_end",
    message: assistantMessage,
  });

  const tokenUsage: TokenUsage = {
    input: assistantMessage.usage.input,
    output: assistantMessage.usage.output,
    total: assistantMessage.usage.totalTokens,
  };

  return { assistantMessage, tokenUsage };
}
