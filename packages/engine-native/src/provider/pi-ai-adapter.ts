import type { AssistantMessage } from "@earendil-works/pi-ai";
import { stream as piStream } from "@earendil-works/pi-ai";
import type { ProviderAdapter, ProviderAdapterResult } from "./types.js";

/**
 * Normalized pi-ai provider adapter.
 *
 * Maps pi-ai stream events (`text_delta`, `thinking_delta`, `toolcall_start`, `done`, `error`)
 * to normalized `EngineProviderEvent` events so that the state machine never inspects
 * provider-specific event names.
 */
export const piAiAdapter: ProviderAdapter = {
  async stream(model, context, options, emit, signal?): Promise<ProviderAdapterResult> {
    const providerOptions: Record<string, unknown> = {};
    if (options.apiKey) providerOptions.apiKey = options.apiKey;
    if (options.headers) providerOptions.headers = options.headers;
    if (signal) providerOptions.signal = signal;
    if (options.reasoning) providerOptions.reasoning = options.reasoning;

    const piContext = {
      systemPrompt: context.systemPrompt,
      messages: context.messages,
      tools: context.tools?.map((t) => ({
        name: t.name,
        description: t.description,
        parameters: t.parameters as never,
      })),
    };

    emit({
      type: "provider_request_start",
      provider: model.provider,
      model: model.id,
    });

    try {
      const s = piStream(model, piContext, providerOptions);
      let piAssistantMessage: AssistantMessage | undefined;
      let streamError = false;

      for await (const event of s) {
        if (event.type === "text_delta") {
          emit({
            type: "provider_text_delta",
            messageId: "assistant",
            delta: event.delta,
          });
        } else if (event.type === "thinking_delta") {
          emit({
            type: "provider_thinking_delta",
            messageId: "assistant",
            delta: event.delta,
          });
        } else if (event.type === "toolcall_start") {
          const tc = event.partial.content[event.contentIndex];
          if (tc?.type === "toolCall") {
            emit({
              type: "provider_tool_call_delta",
              id: tc.id,
              name: tc.name,
              argsDelta: undefined,
            });
          }
        } else if (event.type === "done") {
          piAssistantMessage = event.message;
        } else if (event.type === "error") {
          streamError = true;
          piAssistantMessage = event.error;
          emit({
            type: "provider_error",
            message: "Provider returned an error",
            retryable: false,
          });
        }
      }

      if (!piAssistantMessage) {
        emit({
          type: "provider_error",
          message: "No response from provider",
          retryable: false,
        });
        return { messages: [], usage: createEmptyUsage(), isError: true };
      }

      const usage = fromPiUsage(piAssistantMessage.usage);
      emit({
        type: "provider_message_end",
        message: piAssistantMessage,
        usage,
      });

      return {
        messages: [piAssistantMessage],
        usage,
        isError: streamError,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      emit({
        type: "provider_error",
        message,
        retryable: false,
      });
      return { messages: [], usage: createEmptyUsage(), isError: true };
    }
  },
};

function createEmptyUsage() {
  return {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
  };
}

function fromPiUsage(usage: AssistantMessage["usage"]) {
  return {
    input: usage.input,
    output: usage.output,
    cacheRead: usage.cacheRead,
    cacheWrite: usage.cacheWrite,
    totalTokens: usage.totalTokens,
    cost: {
      input: usage.cost.input,
      output: usage.cost.output,
      cacheRead: usage.cost.cacheRead,
      cacheWrite: usage.cost.cacheWrite,
      total: usage.cost.total,
    },
  };
}
