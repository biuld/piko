import type { Api, Model } from "@earendil-works/pi-ai";
import type { EngineProviderConfig } from "piko-protocol";
import { getEnvApiKey, getModel, getModels, getProviders, type KnownProvider } from "piko-protocol";

export function listAvailableModels(): {
  provider: string;
  models: { id: string; name: string }[];
}[] {
  const providers = getProviders();
  return providers
    .map((provider) => {
      const models = getModels(provider as KnownProvider);
      return { provider, models: models.map((m) => ({ id: m.id, name: m.name })) };
    })
    .filter((p) => p.models.length > 0);
}

export function findModel(
  modelId?: string,
  providerName?: string,
): { model: Model<Api>; providerConfig: EngineProviderConfig } | null {
  const providers = getProviders();

  if (modelId && providerName) {
    // Try the specified provider first (fix #3)
    try {
      const m = getModel(providerName as KnownProvider, modelId as never);
      if (m) return toResult(m);
    } catch {
      /* not found under this provider */
    }
    // Fall back to scanning all providers
    for (const p of providers) {
      if (p === providerName) continue;
      try {
        const m = getModel(p as KnownProvider, modelId as never);
        if (m) return toResult(m);
      } catch {
        /* not found */
      }
    }
  } else if (modelId) {
    for (const p of providers) {
      try {
        const m = getModel(p as KnownProvider, modelId as never);
        if (m) return toResult(m);
      } catch {
        /* not found */
      }
    }
  }

  if (providerName) {
    const models = getModels(providerName as KnownProvider);
    if (models.length > 0) return toResult(models[0]);
  }

  const defaults = [
    { provider: "anthropic", model: "claude-sonnet-4-5-20250929" },
    { provider: "openai", model: "gpt-4o" },
  ];
  for (const d of defaults) {
    try {
      const m = getModel(d.provider as KnownProvider, d.model as never);
      if (m) return toResult(m);
    } catch {
      /* not available */
    }
  }

  for (const p of providers) {
    const models = getModels(p as KnownProvider);
    if (models.length > 0) return toResult(models[0]);
  }

  return null;
}

function toResult(piModel: Model<Api>): {
  model: Model<Api>;
  providerConfig: EngineProviderConfig;
} {
  return {
    model: piModel,
    providerConfig: {
      apiKey: getEnvApiKey(piModel.provider) ?? undefined,
      baseUrl: piModel.baseUrl,
    },
  };
}
