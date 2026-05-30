import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig, EngineRunSettings } from "piko-engine-protocol";

export interface HostConfig {
  model: Model<string>;
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
  model: Model<string>,
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
