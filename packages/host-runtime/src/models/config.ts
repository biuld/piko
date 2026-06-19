import type { Model } from "@earendil-works/pi-ai";
import type { ModelProviderConfig, ModelRunSettings, ToolDef } from "piko-orchestrator-protocol";

export interface HostConfig {
  model: Model<string>;
  provider: ModelProviderConfig;
  settings: ModelRunSettings;
  /** Registered tools (all tools, regardless of active status). */
  tools?: ToolDef[];
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
