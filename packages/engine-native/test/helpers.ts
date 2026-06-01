import type {
  EngineEvent,
  EngineRunSettings,
  EventStream,
  Model,
  TokenUsage,
} from "piko-engine-protocol";
import type {
  ProviderAdapter,
  ProviderAdapterResult,
  ProviderContext,
} from "../src/provider/types.js";

export const emptyUsage: TokenUsage = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};

export function makeModel(): Model<string> {
  return {
    id: "test-model",
    name: "Test Model",
    api: "openai-completions" as const,
    provider: "openai",
    baseUrl: "https://api.openai.com/v1",
    reasoning: false,
  };
}

export function makeSettings(overrides?: Partial<EngineRunSettings>): EngineRunSettings {
  return {
    maxSteps: 10,
    allowToolCalls: true,
    allowApprovals: true,
    ...overrides,
  };
}

export function makeFauxAdapter(
  handler: (context: ProviderContext) => ProviderAdapterResult,
): ProviderAdapter {
  return {
    async stream(_model, context, _options, emit): Promise<ProviderAdapterResult> {
      emit({
        type: "provider_request_start",
        provider: "test",
        model: "test-model",
      });
      const result = handler(context);
      const msg = result.messages[0] ?? {
        role: "assistant" as const,
        content: [],
        api: "openai-completions" as const,
        provider: "test",
        model: "test-model",
        usage: emptyUsage,
        stopReason: "stop" as const,
        timestamp: Date.now(),
      };
      emit({
        type: "provider_message_end",
        message: msg,
        usage: result.usage,
      });
      return { isError: false, ...result };
    },
  };
}

export function collectEvents(stream: EventStream<EngineEvent, unknown>): Promise<EngineEvent[]> {
  const events: EngineEvent[] = [];
  return new Promise((resolve, reject) => {
    void (async () => {
      try {
        for await (const event of stream) {
          events.push(event);
        }
        resolve(events);
      } catch (err) {
        reject(err);
      }
    })();
  });
}
