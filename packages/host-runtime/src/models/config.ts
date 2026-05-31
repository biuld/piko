import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig, EngineRunSettings, EngineTool } from "piko-engine-protocol";

export interface HostConfig {
  model: Model<string>;
  provider: EngineProviderConfig;
  settings: EngineRunSettings;
  /** Registered tools (all tools, regardless of active status). */
  tools?: EngineTool[];
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
  tools?: EngineTool[],
): HostConfig {
  return {
    model,
    provider: {
      ...providerOverrides,
    },
    settings: createDefaultSettings(settingsOverrides),
    tools,
  };
}
