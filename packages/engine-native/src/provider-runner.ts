import type { AssistantMessage } from "@earendil-works/pi-ai";
import type { EngineEvent, EngineInput, TokenUsage } from "piko-engine-protocol";
import { piAiAdapter } from "./provider/pi-ai-adapter.js";
import type { ProviderAdapter, ProviderTool } from "./provider/types.js";
import { buildErrorMessage } from "./transcript-builder.js";

export interface ProviderResult {
  assistantMessage: AssistantMessage;
  tokenUsage: TokenUsage;
  /** True when the provider call failed (network error, no response, etc.) */
  isError: boolean;
}

export async function runProviderCall(
  input: EngineInput,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
  adapter: ProviderAdapter = piAiAdapter,
): Promise<ProviderResult> {
  const { model, provider, transcript, systemPrompt, tools, settings } = input;

  const providerTools: ProviderTool[] | undefined =
    tools && tools.length > 0
      ? tools.map((t) => ({
          name: t.name,
          description: t.description,
          parameters: t.inputSchema,
        }))
      : undefined;

  // Map normalized provider events to Engine events.
  // All provider events are now part of the EngineEvent union,
  // so we forward them directly.
  const providerEmit = (event: import("piko-engine-protocol").EngineProviderEvent) => {
    switch (event.type) {
      case "provider_text_delta":
        emit({ type: "message_delta", messageId: event.messageId, delta: event.delta });
        break;
      case "provider_thinking_delta":
        emit({
          type: "thinking_delta",
          messageId: event.messageId,
          delta: event.delta,
        });
        break;
      case "provider_tool_call_delta":
        emit(event);
        break;
      default:
        // Forward all other provider events directly (provider_request_start,
        // provider_response_start, provider_message_end, provider_error)
        emit(event);
        break;
    }
  };

  const result = await adapter.stream(
    model,
    {
      systemPrompt,
      messages: transcript,
      tools: providerTools,
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
    providerEmit,
    signal,
  );

  // Check for adapter-level error first (takes priority over role checking)
  if (result.isError) {
    const errMsg =
      result.messages.find((m) => m.role === "assistant") ?? buildErrorMessage("Provider error");
    return {
      assistantMessage: errMsg as AssistantMessage,
      tokenUsage: result.usage,
      isError: true,
    };
  }

  const assistantMessage = result.messages.find((m) => m.role === "assistant") as
    | AssistantMessage
    | undefined;

  if (!assistantMessage) {
    const errMsg = buildErrorMessage(
      result.messages.length === 0
        ? "No response from provider"
        : "No assistant message from provider",
    );
    return {
      assistantMessage: errMsg,
      tokenUsage: result.usage,
      isError: true,
    };
  }

  emit({ type: "message_end", message: assistantMessage });

  return {
    assistantMessage,
    tokenUsage: result.usage,
    isError: false,
  };
}
