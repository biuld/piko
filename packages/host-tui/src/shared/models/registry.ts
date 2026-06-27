/**
 * Model Registry — manages model discovery and provider configuration.
 *
 * Wraps pi-ai's model discovery with:
 * - Persisted provider configs (API keys, custom URLs)
 * - Model filtering (scoped models)
 * - Auth integration
 */

import type { Api, Model as PiModel } from "@earendil-works/pi-ai";
import {
  getEnvApiKey,
  getModel,
  getModels,
  getProviders,
  type KnownProvider,
} from "@earendil-works/pi-ai";
import type { AuthStorage } from "../auth/index.js";
import type { Model, ModelProviderConfig } from "../orchd/protocol/index.js";

// ============================================================================
// Types
// ============================================================================

export interface ProviderInfo {
  provider: string;
  models: { id: string; name: string }[];
}

export interface ResolvedModel {
  model: Model<string>;
  providerConfig: ModelProviderConfig;
}

export interface CustomProviderConfig {
  provider: string;
  models: Model<string>[];
}

// ============================================================================
// Registry
// ============================================================================

export class ModelRegistry {
  private authStorage: AuthStorage;
  private scopedModels: string[];
  private customProviders = new Map<string, Model<string>[]>();

  constructor(authStorage: AuthStorage, scopedModels: string[] = []) {
    this.authStorage = authStorage;
    this.scopedModels = scopedModels;
  }

  /** Update scoped model patterns at runtime (e.g. after /model scope change). */
  setScopedModels(patterns: string[]): void {
    this.scopedModels = patterns;
  }

  /**
   * Register a custom provider with its models.
   * Models from custom providers appear alongside built-in models.
   */
  registerCustomProvider(providerId: string, models: Model<string>[]): void {
    this.customProviders.set(providerId, models);
  }

  /** Get all custom provider IDs. */
  getCustomProviderIds(): string[] {
    return Array.from(this.customProviders.keys());
  }

  // ---- Discovery ----

  listProviders(): ProviderInfo[] {
    const providers = getProviders();
    const builtIn: ProviderInfo[] = providers
      .map((provider) => {
        const models = getModels(provider as KnownProvider);
        return {
          provider,
          models: models.map((m) => ({ id: m.id, name: m.name })),
        };
      })
      .filter((p) => p.models.length > 0);

    const custom: ProviderInfo[] = [];
    for (const [provider, models] of this.customProviders) {
      custom.push({
        provider,
        models: models.map((m) => ({ id: m.id, name: m.name })),
      });
    }

    return [...builtIn, ...custom];
  }

  listModels(): Model<string>[] {
    const providers = getProviders();
    const models: Model<string>[] = [];
    for (const p of providers) {
      try {
        const providerModels = getModels(p as KnownProvider);
        for (const m of providerModels) {
          models.push(m as Model<string>);
        }
      } catch {
        // Skip providers with no models
      }
    }

    // Add custom provider models
    for (const customModels of this.customProviders.values()) {
      models.push(...customModels);
    }

    return models;
  }

  listScopedModels(): Model<string>[] {
    if (this.scopedModels.length === 0) return this.listModels();

    const allModels = this.listModels();
    const matching: Model<string>[] = [];

    for (const pattern of this.scopedModels) {
      // Patterns: "provider" or "provider/model"
      const [prov, modelId] = pattern.includes("/") ? pattern.split("/") : [pattern, undefined];

      for (const m of allModels) {
        const providerMatch = !prov || m.provider.toLowerCase() === prov.toLowerCase();
        const modelMatch = !modelId || m.id.toLowerCase().includes(modelId.toLowerCase());

        if (providerMatch && modelMatch) {
          if (
            !matching.some((existing) => existing.provider === m.provider && existing.id === m.id)
          ) {
            matching.push(m);
          }
        }
      }
    }

    return matching;
  }

  // ---- Resolve ----

  resolve(modelId?: string, providerName?: string): ResolvedModel | null {
    const providers = getProviders();

    // Check custom providers first
    if (providerName && this.customProviders.has(providerName)) {
      const customModels = this.customProviders.get(providerName)!;
      if (modelId) {
        const m = customModels.find((cm) => cm.id === modelId);
        if (m) return this.toResolved(m);
      }
      if (customModels.length > 0) return this.toResolved(customModels[0]);
    }

    if (modelId && providerName) {
      // Try the specified provider first (fix #1)
      try {
        const m = getModel(providerName as KnownProvider, modelId as never);
        if (m) return this.toResolved(m);
      } catch {
        /* not found under this provider */
      }
      // Fall back to scanning all providers
      for (const p of providers) {
        if (p === providerName) continue; // already tried
        try {
          const m = getModel(p as KnownProvider, modelId as never);
          if (m) return this.toResolved(m);
        } catch {
          /* not found */
        }
      }

      // Also check all custom providers
      for (const [cp, customModels] of this.customProviders) {
        if (cp === providerName) continue; // already checked
        const m = customModels.find((cm) => cm.id === modelId);
        if (m) return this.toResolved(m);
      }
    } else if (modelId) {
      for (const p of providers) {
        try {
          const m = getModel(p as KnownProvider, modelId as never);
          if (m) return this.toResolved(m);
        } catch {
          /* not found */
        }
      }
      // Check custom providers
      for (const customModels of this.customProviders.values()) {
        const m = customModels.find((cm) => cm.id === modelId);
        if (m) return this.toResolved(m);
      }
    }

    if (providerName) {
      const models = getModels(providerName as KnownProvider);
      if (models.length > 0) return this.toResolved(models[0]);
    }

    // Defaults
    const defaults: Array<{ provider: string; model: string }> = [
      { provider: "anthropic", model: "claude-sonnet-4-5-20250929" },
      { provider: "openai", model: "gpt-4o" },
    ];

    for (const d of defaults) {
      try {
        const m = getModel(d.provider as KnownProvider, d.model as never);
        if (m) return this.toResolved(m);
      } catch {
        /* not available */
      }
    }

    // First available
    for (const p of providers) {
      const models = getModels(p as KnownProvider);
      if (models.length > 0) return this.toResolved(models[0]);
    }

    // First custom provider
    for (const customModels of this.customProviders.values()) {
      if (customModels.length > 0) return this.toResolved(customModels[0]);
    }

    return null;
  }

  private toResolved(piModel: Model | PiModel<Api>): ResolvedModel {
    return {
      model: piModel,
      providerConfig: {
        apiKey:
          this.authStorage.getApiKey(piModel.provider) ??
          getEnvApiKey(piModel.provider) ??
          undefined,
        baseUrl: (piModel as { baseUrl?: string }).baseUrl,
      },
    };
  }

  // ---- Auth ----

  hasAuth(provider: string): boolean {
    return this.authStorage.hasAuth(provider);
  }

  getAuthStorage(): AuthStorage {
    return this.authStorage;
  }
}
