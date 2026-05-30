import type {
  Message,
  EngineModel,
  EngineProviderConfig,
  EngineTool,
  TokenUsage,
  EventStream,
} from "piko-engine-protocol";

export interface LlmCallInput {
  model: EngineModel;
  provider: EngineProviderConfig;
  transcript: Message[];
  systemPrompt: string;
  tools: EngineTool[];
}

export type LlmEvent =
  | { type: "text_delta"; delta: string }
  | { type: "tool_call_start"; id: string; name: string };

export interface LlmResult {
  message: Message;
  usage: TokenUsage;
}

export interface LlmCaller {
  call(input: LlmCallInput, signal?: AbortSignal): EventStream<LlmEvent, LlmResult>;
}
