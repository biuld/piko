import type { Model, ModelProviderConfig, ModelRunSettings } from "piko-orch-protocol";

export interface HostConfig {
  model: Model<string>;
  provider: ModelProviderConfig;
  settings: ModelRunSettings;
}

const DEFAULT_SETTINGS: ModelRunSettings = {
  parallelTools: true,
  allowToolCalls: true,
  runtimeLimits: {
    perToolTimeoutMs: 120_000, // 2 minutes per tool
    maxConsecutiveErrors: 5,
  },
};

export function createDefaultSettings(overrides?: Partial<ModelRunSettings>): ModelRunSettings {
  return { ...DEFAULT_SETTINGS, ...overrides };
}

export function createHostConfig(
  model: Model<string>,
  providerOverrides?: Partial<ModelProviderConfig>,
  settingsOverrides?: Partial<ModelRunSettings>,
): HostConfig {
  return {
    model,
    provider: {
      ...providerOverrides,
    },
    settings: createDefaultSettings(settingsOverrides),
  };
}
