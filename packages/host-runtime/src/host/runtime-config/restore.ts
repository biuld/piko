import type { HostConfig, ModelRegistry } from "../../models/index.js";
import type { SessionManager, SessionPersistenceOverview } from "../../session/index.js";

export interface RestoreRuntimeFromSessionResult {
  config: HostConfig | null;
  thinkingLevel: string | undefined;
  activeToolNames: string[] | undefined;
  /** true when an active_tools_change entry was found (even if empty/clear). */
  hasActiveToolsEntry: boolean;
  sessionPersistenceOverview?: SessionPersistenceOverview;
}

export async function restoreRuntimeFromSession(
  sessionManager: SessionManager,
  currentConfig: HostConfig,
  modelRegistry?: ModelRegistry,
): Promise<RestoreRuntimeFromSessionResult> {
  try {
    const entries = await sessionManager.getBranch();
    const sessionPersistenceOverview = await loadSessionPersistenceOverview(sessionManager);
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
      if (modelRegistry) {
        const resolved = modelRegistry.resolve(lastModel.modelId, lastModel.provider);
        if (resolved) {
          config = {
            model: resolved.model,
            provider: resolved.providerConfig,
            settings: currentConfig.settings,
          };
        }
      }
      if (!config) {
        config = await resolveModelConfig(lastModel.modelId, lastModel.provider, currentConfig);
      }
    }

    return {
      config,
      thinkingLevel: lastThinking,
      activeToolNames: lastActiveTools,
      hasActiveToolsEntry: foundActiveToolsEntry,
      sessionPersistenceOverview,
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

async function loadSessionPersistenceOverview(
  sessionManager: SessionManager,
): Promise<SessionPersistenceOverview | undefined> {
  if (typeof sessionManager.loadPersistenceOverview !== "function") return undefined;
  try {
    return await sessionManager.loadPersistenceOverview();
  } catch {
    return undefined;
  }
}

async function resolveModelConfig(
  modelId: string,
  provider: string,
  currentConfig: HostConfig,
): Promise<HostConfig | null> {
  try {
    const { getModel } = await import("piko-orchestrator");
    const { AuthStorage } = await import("../../auth/index.js");
    const m = getModel(provider as any, modelId as never);
    if (m) {
      const authStorage = await AuthStorage.create();
      const apiKey =
        !currentConfig.model || provider === currentConfig.model.provider
          ? (currentConfig.provider.apiKey ?? authStorage.getApiKey(provider))
          : authStorage.getApiKey(provider);

      return {
        model: m,
        provider: {
          apiKey,
          baseUrl: (m as any).baseUrl,
        },
        settings: currentConfig.settings,
      };
    }
  } catch {
    /* ignore */
  }
  return null;
}
