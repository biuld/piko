import type { Context, Api } from "@earendil-works/pi-ai";
import type { AssistantMessage as PiAssistantMessage } from "@earendil-works/pi-ai";
import { stream as piStream } from "@earendil-works/pi-ai";
import { EventStream } from "piko-engine-protocol";
import type {
  Message,
  EngineModel,
  EngineTool,
  TokenUsage,
} from "piko-engine-protocol";
import type { LlmCaller, LlmCallInput, LlmEvent, LlmResult } from "piko-engine-native";
import { toPiMessage, toPiModel, fromPiAssistantMessage, fromPiUsage } from "./bridge.js";

export function createPiLlmCaller(): LlmCaller {
  return {
    call(input: LlmCallInput, signal?: AbortSignal): EventStream<LlmEvent, LlmResult> {
      const eventStream = new EventStream<LlmEvent, LlmResult>();

      void runPiCall(eventStream, input, signal)
        .then((result) => eventStream.end(result))
        .catch((err) => {
          const errorMsg = err instanceof Error ? err.message : String(err);
          eventStream.end({
            message: {
              role: "assistant",
              content: [{ type: "text", text: errorMsg }],
              timestamp: Date.now(),
            },
            usage: { input: 0, output: 0, total: 0 },
          });
        });

      return eventStream;
    },
  };
}

async function runPiCall(
  eventStream: EventStream<LlmEvent, LlmResult>,
  input: LlmCallInput,
  signal?: AbortSignal,
): Promise<LlmResult> {
  const { model, provider, transcript, systemPrompt, tools } = input;

  const piModel = toPiModel(model);

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
  if (provider.reasoning?.effort) {
    providerOptions.reasoning = provider.reasoning.effort;
  }

  let piAssistantMessage: PiAssistantMessage | undefined;

  try {
    const s = piStream(piModel, context, providerOptions);

    for await (const event of s) {
      if (event.type === "text_delta") {
        eventStream.push({ type: "text_delta", delta: event.delta });
      } else if (event.type === "toolcall_start") {
        const tc = event.partial.content[event.contentIndex];
        if (tc?.type === "toolCall") {
          eventStream.push({ type: "tool_call_start", id: tc.id, name: tc.name });
        }
      } else if (event.type === "done") {
        piAssistantMessage = event.message;
      } else if (event.type === "error") {
        piAssistantMessage = event.error;
      }
    }

    if (!piAssistantMessage) {
      return {
        message: {
          role: "assistant",
          content: [{ type: "text", text: "No response from provider" }],
          timestamp: Date.now(),
        },
        usage: { input: 0, output: 0, total: 0 },
      };
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      message: {
        role: "assistant",
        content: [{ type: "text", text: message }],
        timestamp: Date.now(),
      },
      usage: { input: 0, output: 0, total: 0 },
    };
  }

  return {
    message: fromPiAssistantMessage(piAssistantMessage),
    usage: fromPiUsage(piAssistantMessage.usage),
  };
}
