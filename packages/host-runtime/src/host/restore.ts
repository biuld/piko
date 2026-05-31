import type { HostConfig } from "../models/index.js";
import type { SessionManager } from "../session/index.js";

export async function restoreRuntimeFromSession(
  sessionManager: SessionManager,
  currentConfig: HostConfig,
): Promise<{ config: HostConfig | null; thinkingLevel: string | undefined }> {
  try {
    const entries = await sessionManager.getBranch();
    let lastModel: { provider: string; modelId: string } | undefined;
    let lastThinking: string | undefined;
    for (let i = entries.length - 1; i >= 0; i--) {
      const e = entries[i];
      if (!lastModel && e.type === "model_change") {
        lastModel = { provider: (e as any).provider, modelId: (e as any).modelId };
      }
      if (!lastThinking && e.type === "thinking_level_change") {
        lastThinking = (e as any).thinkingLevel;
      }
      if (lastModel && lastThinking) break;
    }

    let config: HostConfig | null = null;
    if (lastModel) {
      config = await resolveModelConfig(lastModel.modelId, lastModel.provider, currentConfig);
    }

    return { config, thinkingLevel: lastThinking };
  } catch {
    return { config: null, thinkingLevel: undefined };
  }
}

async function resolveModelConfig(
  modelId: string,
  provider: string,
  currentConfig: HostConfig,
): Promise<HostConfig | null> {
  try {
    const { getModel, getEnvApiKey } = await import("piko-engine-protocol");
    const m = getModel(provider as any, modelId as never);
    if (m) {
      return {
        model: m,
        provider: {
          apiKey: currentConfig.provider.apiKey ?? getEnvApiKey(provider) ?? undefined,
          baseUrl: (m as any).baseUrl,
        },
        settings: currentConfig.settings,
      };
    }
  } catch { /* ignore */ }
  return null;
}
