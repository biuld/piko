import {
  getProviders,
  getModels,
  getModel,
} from "@earendil-works/pi-ai";
import type { KnownProvider } from "@earendil-works/pi-ai";
import { getEnvApiKey } from "@earendil-works/pi-ai";
import type { EngineModel, EngineProviderConfig } from "piko-engine-protocol";

export function listAvailableModels(): { provider: string; models: { id: string; name: string }[] }[] {
  const providers = getProviders();
  return providers.map((provider) => {
    const models = getModels(provider as KnownProvider);
    return {
      provider,
      models: models.map((m) => ({ id: m.id, name: m.name })),
    };
  }).filter((p) => p.models.length > 0);
}

export function findModel(
  modelId?: string,
  providerName?: string,
): { model: EngineModel; providerConfig: EngineProviderConfig } | null {
  const providers = getProviders();

  // Try to find by model ID across all providers
  if (modelId) {
    for (const p of providers) {
      try {
        const m = getModel(p as KnownProvider, modelId as never);
        if (m) {
          return toEngineModelWithConfig(m);
        }
      } catch {
        // Model not found for this provider
      }
    }
  }

  // Try to find by provider name — use first model
  if (providerName) {
    const models = getModels(providerName as KnownProvider);
    if (models.length > 0) {
      return toEngineModelWithConfig(models[0]);
    }
  }

  // Default: try anthropic claude-sonnet-4-5, then openai gpt-4o, then first available
  const defaults = [
    { provider: "anthropic", model: "claude-sonnet-4-5-20250929" },
    { provider: "openai", model: "gpt-4o" },
  ];

  for (const d of defaults) {
    try {
      const m = getModel(d.provider as KnownProvider, d.model as never);
      if (m) {
        return toEngineModelWithConfig(m);
      }
    } catch {
      // Not available
    }
  }

  // Fallback: first model from first provider
  for (const p of providers) {
    const models = getModels(p as KnownProvider);
    if (models.length > 0) {
      return toEngineModelWithConfig(models[0]);
    }
  }

  return null;
}

function toEngineModelWithConfig(piModel: {
  id: string;
  name: string;
  api: string;
  provider: string;
  baseUrl: string;
  reasoning: boolean;
  input: ("text" | "image")[];
  contextWindow: number;
  maxTokens: number;
}): { model: EngineModel; providerConfig: EngineProviderConfig } {
  const model: EngineModel = {
    id: piModel.id,
    name: piModel.name,
    api: piModel.api,
    provider: piModel.provider,
    baseUrl: piModel.baseUrl,
    reasoning: piModel.reasoning,
    input: piModel.input,
    contextWindow: piModel.contextWindow,
    maxTokens: piModel.maxTokens,
  };

  const apiKey = getEnvApiKey(piModel.provider);

  const providerConfig: EngineProviderConfig = {
    apiKey: apiKey ?? undefined,
    baseUrl: piModel.baseUrl,
  };

  return { model, providerConfig };
}
