import type { Context } from "@earendil-works/pi-ai";
import { stream } from "@earendil-works/pi-ai";
import type {
  EngineInput,
  EngineEvent,
  Message,
  TokenUsage,
} from "piko-engine-protocol";
import { toPiMessage, toPiModel, fromPiAssistantMessage, fromPiUsage } from "./bridge.js";
import { buildErrorMessage } from "./transcript-builder.js";

export interface ProviderResult {
  assistantMessage: Message;
  tokenUsage: TokenUsage;
}

export async function runProviderCall(
  input: EngineInput,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
): Promise<ProviderResult> {
  const { model, provider, transcript, systemPrompt, tools } = input;

  const piModel = toPiModel(model);

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
    messages: transcript.map(toPiMessage),
    tools: piTools,
  };

  const providerOptions: Record<string, unknown> = {};
  if (provider.apiKey) providerOptions.apiKey = provider.apiKey;
  if (provider.headers) providerOptions.headers = provider.headers;
  if (signal) providerOptions.signal = signal;

  if (provider.reasoning) {
    providerOptions.reasoning = provider.reasoning.effort;
  }

  emit({ type: "step_start" });

  let piAssistantMessage;

  try {
    const eventStream = stream(piModel, context, providerOptions);

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
        piAssistantMessage = event.message;
      } else if (event.type === "error") {
        piAssistantMessage = event.error;
      }
    }

    if (!piAssistantMessage!) {
      return {
        assistantMessage: buildErrorMessage("No response from provider"),
        tokenUsage: { input: 0, output: 0, total: 0 },
      };
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      assistantMessage: buildErrorMessage(message),
      tokenUsage: { input: 0, output: 0, total: 0 },
    };
  }

  const assistantMessage = fromPiAssistantMessage(piAssistantMessage);
  const tokenUsage = fromPiUsage(piAssistantMessage.usage);

  emit({
    type: "message_end",
    message: assistantMessage,
  });

  return { assistantMessage, tokenUsage };
}
