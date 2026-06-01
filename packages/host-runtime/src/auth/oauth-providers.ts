/**
 * OAuth Provider Registry.
 *
 * Source: @earendil-works/pi-ai/src/utils/oauth/index.ts
 */

import { anthropicOAuthProvider } from "./anthropic.js";
import { githubCopilotOAuthProvider } from "./github-copilot.js";
import { openaiCodexOAuthProvider } from "./openai-codex.js";
import type { OAuthCredentials, OAuthProviderId, OAuthProviderInfo, OAuthProviderInterface } from "./oauth-types.js";

const BUILT_IN_OAUTH_PROVIDERS: OAuthProviderInterface[] = [
  anthropicOAuthProvider,
  githubCopilotOAuthProvider,
  openaiCodexOAuthProvider,
];

const oauthProviderRegistry = new Map<string, OAuthProviderInterface>(
  BUILT_IN_OAUTH_PROVIDERS.map((provider) => [provider.id, provider]),
);

export function getOAuthProvider(id: OAuthProviderId): OAuthProviderInterface | undefined {
  return oauthProviderRegistry.get(id);
}

export function registerOAuthProvider(provider: OAuthProviderInterface): void {
  oauthProviderRegistry.set(provider.id, provider);
}

export function unregisterOAuthProvider(id: string): void {
  const builtInProvider = BUILT_IN_OAUTH_PROVIDERS.find((provider) => provider.id === id);
  if (builtInProvider) {
    oauthProviderRegistry.set(id, builtInProvider);
    return;
  }
  oauthProviderRegistry.delete(id);
}

export function resetOAuthProviders(): void {
  oauthProviderRegistry.clear();
  for (const provider of BUILT_IN_OAUTH_PROVIDERS) {
    oauthProviderRegistry.set(provider.id, provider);
  }
}

export function getOAuthProviders(): OAuthProviderInterface[] {
  return Array.from(oauthProviderRegistry.values());
}

/** @deprecated Use getOAuthProviders() which returns OAuthProviderInterface[] */
export function getOAuthProviderInfoList(): OAuthProviderInfo[] {
  return getOAuthProviders().map((p) => ({
    id: p.id,
    name: p.name,
    available: true,
  }));
}

export async function refreshOAuthToken(
  providerId: OAuthProviderId,
  credentials: OAuthCredentials,
): Promise<OAuthCredentials> {
  const provider = getOAuthProvider(providerId);
  if (!provider) {
    throw new Error(`Unknown OAuth provider: ${providerId}`);
  }
  return provider.refreshToken(credentials);
}

export async function getOAuthApiKey(
  providerId: OAuthProviderId,
  credentials: Record<string, OAuthCredentials>,
): Promise<{ newCredentials: OAuthCredentials; apiKey: string } | null> {
  const provider = getOAuthProvider(providerId);
  if (!provider) {
    throw new Error(`Unknown OAuth provider: ${providerId}`);
  }

  let creds = credentials[providerId];
  if (!creds) {
    return null;
  }

  // Refresh if expired
  if (Date.now() >= creds.expires) {
    try {
      creds = await provider.refreshToken(creds);
    } catch (_error) {
      throw new Error(`Failed to refresh OAuth token for ${providerId}`);
    }
  }

  const apiKey = provider.getApiKey(creds);
  return { newCredentials: creds, apiKey };
}

// Re-export provider implementations for direct use
export {
  anthropicOAuthProvider,
  githubCopilotOAuthProvider,
  openaiCodexOAuthProvider,
};
export { loginAnthropic, refreshAnthropicToken } from "./anthropic.js";
export { loginGitHubCopilot, refreshGitHubCopilotToken } from "./github-copilot.js";
export { loginOpenAICodex, loginOpenAICodexDeviceCode, refreshOpenAICodexToken } from "./openai-codex.js";
