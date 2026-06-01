import type { EngineProviderEvent, Message, Model, TokenUsage } from "piko-engine-protocol";

/**
 * Normalized provider adapter interface.
 * Each provider (pi-ai, OpenAI, Anthropic direct, etc.) implements this.
 */
export interface ProviderAdapter {
  /** Stream a model call, emitting normalized EngineProviderEvent via callback. */
  stream(
    model: Model<string>,
    context: ProviderContext,
    options: ProviderOptions,
    emit: (event: EngineProviderEvent) => void,
    signal?: AbortSignal,
  ): Promise<ProviderAdapterResult>;
}

export interface ProviderContext {
  systemPrompt: string;
  messages: Message[];
  tools?: ProviderTool[];
}

export interface ProviderTool {
  name: string;
  description: string;
  parameters: unknown;
}

export interface ProviderOptions {
  apiKey?: string;
  baseUrl?: string;
  headers?: Record<string, string>;
  reasoning?: string;
  signal?: AbortSignal;
}

export interface ProviderAdapterResult {
  messages: Message[];
  usage: TokenUsage;
  /** True when the provider stream ended with an error. */
  isError: boolean;
}
