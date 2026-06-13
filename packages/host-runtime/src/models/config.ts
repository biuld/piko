import type { Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig, EngineRunSettings, ToolDef } from "piko-protocol";

export interface HostConfig {
  model: Model<string>;
  provider: EngineProviderConfig;
  settings: EngineRunSettings;
  /** Registered tools (all tools, regardless of active status). */
  tools?: ToolDef[];
}

const DEFAULT_SETTINGS: EngineRunSettings = {
  maxSteps: 10,
  parallelTools: true,
  allowToolCalls: true,
  runtimeLimits: {
    perToolTimeoutMs: 120_000, // 2 minutes per tool
    maxConsecutiveErrors: 5,
  },
};

export function createDefaultSettings(overrides?: Partial<EngineRunSettings>): EngineRunSettings {
  return { ...DEFAULT_SETTINGS, ...overrides };
}

export function createHostConfig(
  model: Model<string>,
  providerOverrides?: Partial<EngineProviderConfig>,
  settingsOverrides?: Partial<EngineRunSettings>,
  tools?: ToolDef[],
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
