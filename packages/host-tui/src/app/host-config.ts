import type {
  Model,
  ModelProviderConfig,
  ModelRunSettings,
} from "../shared/orchd/protocol/index.js";

export interface TuiHostConfig {
  model: Model<string>;
  provider: { provider: string; apiKey?: string; baseUrl?: string };
  settings: ModelRunSettings;
}

export function createDefaultSettings(overrides?: Partial<ModelRunSettings>): ModelRunSettings {
  return {
    allowToolCalls: true,
    maxTokens: 16000,
    ...overrides,
  };
}

export function createHostConfig(
  model: Partial<Model<string>> & { id: string; provider: string; name?: string; label?: string },
  providerConfig: ModelProviderConfig,
  settingsOverrides?: Partial<ModelRunSettings>,
): TuiHostConfig {
  const fullModel: Model<string> = {
    id: model.id,
    name: model.name ?? model.label ?? model.id,
    api: model.api ?? model.provider,
    provider: model.provider,
    baseUrl: model.baseUrl ?? providerConfig.baseUrl ?? "",
    reasoning: model.reasoning ?? false,
    input: model.input ?? ["text"],
    cost: model.cost ?? { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: model.contextWindow ?? 0,
    maxTokens: model.maxTokens ?? 16000,
    headers: model.headers,
    compat: model.compat,
    thinkingLevelMap: model.thinkingLevelMap,
  };

  return {
    model: fullModel,
    provider: {
      provider: model.provider,
      apiKey: providerConfig.apiKey,
      baseUrl: providerConfig.baseUrl,
    },
    settings: createDefaultSettings(settingsOverrides),
  };
}
