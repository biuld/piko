/**
 * Provider Registration Framework.
 *
 * Define a provider once (OAuth + models + stream) and call registerProvider().
 * This replaces scattered registration across oauth-providers, ModelRegistry,
 * and pi-ai's registerApiProvider.
 */

import type { AssistantMessageEventStream, Context, Model } from "@earendil-works/pi-ai";
import { registerApiProvider } from "@earendil-works/pi-ai";
import { registerOAuthProvider } from "../../auth/oauth-providers.js";
import type { OAuthProviderInterface } from "../../auth/oauth-types.js";
import type { ModelRegistry } from "../registry.js";

// ============================================================================
// Re-exports
// ============================================================================

export { createAntigravityModels } from "./antigravity-models.js";

// ============================================================================
// Types
// ============================================================================

export type StreamHandler = (
  model: Model<string>,
  context: Context,
  options?: Record<string, unknown>,
) => AssistantMessageEventStream;

export interface ProviderStreamConfig {
  /** API identifier (e.g. "antigravity-api"). Must match model.api. */
  api: string;
  /** Stream handler function. */
  handler: StreamHandler;
}

export interface ProviderDefinition {
  /** Provider ID (e.g. "antigravity"). Must match model.provider and OAuth id. */
  id: string;
  /** OAuth provider for /login support. */
  oauth?: OAuthProviderInterface;
  /** Model definitions for this provider. */
  models: Model<string>[];
  /** Stream API registration. */
  stream?: ProviderStreamConfig;
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
 *     stream: { api: "antigravity-api", handler: streamNoagy },
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

  // 3. Stream
  if (provider.stream) {
    registerApiProvider(
      {
        api: provider.stream.api,
        stream: provider.stream.handler as any,
        streamSimple: provider.stream.handler as any,
      },
      `piko:${provider.id}`,
    );
  }
}
