import type {
  EngineInput,
  EngineEvent,
  Message,
  TokenUsage,
} from "piko-engine-protocol";
import type { LlmCaller } from "./llm-caller.js";
import { buildErrorMessage } from "./transcript-builder.js";

export interface ProviderResult {
  assistantMessage: Message;
  tokenUsage: TokenUsage;
}

export async function runProviderCall(
  input: EngineInput,
  llmCaller: LlmCaller,
  emit: (event: EngineEvent) => void,
  signal?: AbortSignal,
): Promise<ProviderResult> {
  const { model, provider, transcript, systemPrompt, tools } = input;

  emit({ type: "step_start" });

  const llmStream = llmCaller.call(
    { model, provider, transcript, systemPrompt, tools },
    signal,
  );

  try {
    for await (const event of llmStream) {
      switch (event.type) {
        case "text_delta":
          emit({
            type: "message_delta",
            messageId: "assistant",
            delta: event.delta,
          });
          break;
        case "tool_call_start":
          emit({
            type: "tool_call_start",
            id: event.id,
            name: event.name,
            args: {},
          });
          break;
      }
    }

    const result = await llmStream.result();
    const assistantMessage = result.message;

    emit({
      type: "message_end",
      message: assistantMessage,
    });

    return {
      assistantMessage,
      tokenUsage: result.usage,
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      assistantMessage: buildErrorMessage(message),
      tokenUsage: { input: 0, output: 0, total: 0 },
    };
  }
}
