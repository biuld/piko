/**
 * Provider Registration Framework.
 *
 * Define a provider once (OAuth + models + stream) and call registerProvider().
 * This replaces scattered registration across oauth-providers, ModelRegistry,
 * and model registry setup.
 */

import { registerOAuthProvider } from "../../auth/oauth-providers.js";
import type { OAuthProviderInterface } from "../../auth/oauth-types.js";
import type { Model } from "../../orchd/protocol/index.js";
import type { ModelRegistry } from "../registry.js";

// ============================================================================
// Re-exports
// ============================================================================

export { createAntigravityModels } from "./antigravity-models.js";

// ============================================================================
// Types
// ============================================================================

export interface ProviderDefinition {
  /** Provider ID (e.g. "antigravity"). Must match model.provider and OAuth id. */
  id: string;
  /** OAuth provider for /login support. */
  oauth?: OAuthProviderInterface;
  /** Model definitions for this provider. */
  models: Model<string>[];
}

// ============================================================================
// Registration
// ============================================================================

/**
 * Register a complete provider (OAuth + models + stream) in one call.
 *
 * Usage:
 *   registerProvider(modelRegistry, {
 *     id: "antigravity",
 *     oauth: antigravityOAuthProvider,
 *     models: createAntigravityModels(),
 *   });
 */
export function registerProvider(modelRegistry: ModelRegistry, provider: ProviderDefinition): void {
  // 1. OAuth
  if (provider.oauth) {
    registerOAuthProvider(provider.oauth);
  }

  // 2. Models
  if (provider.models.length > 0) {
    modelRegistry.registerCustomProvider(provider.id, provider.models);
  }
}
