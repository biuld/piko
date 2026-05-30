import type { EngineModel, EngineProviderConfig, EngineRunSettings } from "piko-engine-protocol";

export interface HostConfig {
  model: EngineModel;
  provider: EngineProviderConfig;
  settings: EngineRunSettings;
}

const DEFAULT_SETTINGS: EngineRunSettings = {
  maxSteps: 10,
  parallelTools: false,
  allowToolCalls: true,
  allowApprovals: true,
};

export function createDefaultSettings(overrides?: Partial<EngineRunSettings>): EngineRunSettings {
  return { ...DEFAULT_SETTINGS, ...overrides };
}

export function createHostConfig(
  model: EngineModel,
  providerOverrides?: Partial<EngineProviderConfig>,
  settingsOverrides?: Partial<EngineRunSettings>,
): HostConfig {
  return {
    model,
    provider: {
      ...providerOverrides,
    },
    settings: createDefaultSettings(settingsOverrides),
  };
}
