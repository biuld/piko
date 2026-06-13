import type { HostConfig } from "../models/index.js";
import type { SessionManager } from "../session/index.js";

export async function restoreRuntimeFromSession(
  sessionManager: SessionManager,
  currentConfig: HostConfig,
): Promise<{
  config: HostConfig | null;
  thinkingLevel: string | undefined;
  activeToolNames: string[] | undefined;
  /** true when an active_tools_change entry was found (even if empty/clear). */
  hasActiveToolsEntry: boolean;
}> {
  try {
    const entries = await sessionManager.getBranch();
    let lastModel: { provider: string; modelId: string } | undefined;
    let lastThinking: string | undefined;
    let lastActiveTools: string[] | undefined;
    let foundActiveToolsEntry = false;
    let foundModelEntry = false;
    let foundThinkingEntry = false;
    for (let i = entries.length - 1; i >= 0; i--) {
      const e = entries[i];
      if (!foundModelEntry && e.type === "model_change") {
        foundModelEntry = true;
        lastModel = { provider: (e as any).provider, modelId: (e as any).modelId };
      }
      if (!foundThinkingEntry && e.type === "thinking_level_change") {
        foundThinkingEntry = true;
        lastThinking = (e as any).thinkingLevel;
      }
      if (!foundActiveToolsEntry && e.type === "active_tools_change") {
        foundActiveToolsEntry = true;
        const names = (e as any).activeToolNames ?? (e as any).toolNames;
        // Empty array means "all tools active" (explicit clear).
        lastActiveTools = Array.isArray(names) && names.length > 0 ? names : undefined;
      }
      if (foundModelEntry && foundThinkingEntry && foundActiveToolsEntry) break;
    }

    let config: HostConfig | null = null;
    if (lastModel) {
      config = await resolveModelConfig(lastModel.modelId, lastModel.provider, currentConfig);
    }

    return {
      config,
      thinkingLevel: lastThinking,
      activeToolNames: lastActiveTools,
      hasActiveToolsEntry: foundActiveToolsEntry,
    };
  } catch {
    return {
      config: null,
      thinkingLevel: undefined,
      activeToolNames: undefined,
      hasActiveToolsEntry: false,
    };
  }
}

async function resolveModelConfig(
  modelId: string,
  provider: string,
  currentConfig: HostConfig,
): Promise<HostConfig | null> {
  try {
    const { getModel, getEnvApiKey } = await import("piko-protocol");
    const m = getModel(provider as any, modelId as never);
    if (m) {
      return {
        model: m,
        provider: {
          apiKey: currentConfig.provider.apiKey ?? getEnvApiKey(provider) ?? undefined,
          baseUrl: (m as any).baseUrl,
        },
        settings: currentConfig.settings,
        // Preserve tool definitions from current config so active tools
        // filtering survives model_change restores.
        tools: currentConfig.tools,
      };
    }
  } catch {
    /* ignore */
  }
  return null;
}
