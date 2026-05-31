/**
 * OAuth helpers — device-code flow for provider authentication.
 *
 * Currently supports Anthropic and OpenAI OAuth device-code flows.
 * The user opens a URL, enters a code, and the token is saved to AuthStorage.
 */

import type { OAuthCredential } from "./storage.js";

// ============================================================================
// Types
// ============================================================================

export interface OAuthDeviceCodeResponse {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  verificationUriComplete?: string;
  expiresIn: number;
  interval: number;
}

export interface OAuthTokenResponse {
  accessToken: string;
  refreshToken?: string;
  expiresIn?: number;
  tokenType: string;
}

export interface OAuthProviderConfig {
  /** OAuth device authorization endpoint. */
  deviceAuthorizationEndpoint: string;
  /** OAuth token endpoint. */
  tokenEndpoint: string;
  /** OAuth client ID. */
  clientId: string;
  /** Optional scopes. */
  scopes?: string[];
}

// ============================================================================
// Provider configs
// ============================================================================

const ANTHROPIC_CONFIG: OAuthProviderConfig = {
  deviceAuthorizationEndpoint: "https://api.anthropic.com/oauth/device/authorize",
  tokenEndpoint: "https://api.anthropic.com/oauth/token",
  clientId: "piko-cli",
  scopes: ["anthropic:models:read", "anthropic:messages:write"],
};

const OPENAI_CONFIG: OAuthProviderConfig = {
  deviceAuthorizationEndpoint: "https://auth.openai.com/oauth/device/authorize",
  tokenEndpoint: "https://auth.openai.com/oauth/token",
  clientId: "piko-cli",
  scopes: ["openid", "offline_access", "model.read"],
};

export function getOAuthConfig(provider: string): OAuthProviderConfig | undefined {
  const lower = provider.toLowerCase();
  if (lower === "anthropic") return ANTHROPIC_CONFIG;
  if (lower === "openai") return OPENAI_CONFIG;
  return undefined;
}

// ============================================================================
// Device code flow
// ============================================================================

/**
 * Step 1: Request a device code from the provider.
 */
export async function requestDeviceCode(
  config: OAuthProviderConfig,
): Promise<OAuthDeviceCodeResponse> {
  const body = new URLSearchParams({
    client_id: config.clientId,
    scope: (config.scopes ?? []).join(" "),
  });

  const response = await fetch(config.deviceAuthorizationEndpoint, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: body.toString(),
  });

  if (!response.ok) {
    throw new Error(`OAuth device authorization failed: ${response.status} ${response.statusText}`);
  }

  return response.json() as Promise<OAuthDeviceCodeResponse>;
}

/**
 * Step 2: Poll for token completion.
 * Returns the token response once the user has authorized, or throws on timeout/error.
 */
export async function pollForToken(
  config: OAuthProviderConfig,
  deviceCode: string,
  intervalMs: number,
  timeoutMs: number,
): Promise<OAuthTokenResponse> {
  const start = Date.now();

  while (Date.now() - start < timeoutMs) {
    const body = new URLSearchParams({
      grant_type: "urn:ietf:params:oauth:grant-type:device_code",
      device_code: deviceCode,
      client_id: config.clientId,
    });

    const response = await fetch(config.tokenEndpoint, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: body.toString(),
    });

    if (response.ok) {
      return response.json() as Promise<OAuthTokenResponse>;
    }

    // "authorization_pending" = user hasn't authorized yet, keep polling
    // "slow_down" = polling too fast, increase interval
    const data = await response.json().catch(() => ({}));
    if (data.error === "slow_down") {
      intervalMs = Math.min(intervalMs * 2, 30000);
    } else if (data.error !== "authorization_pending") {
      throw new Error(`OAuth token request failed: ${data.error ?? response.status}`);
    }

    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }

  throw new Error("OAuth device code expired (timeout)");
}

/**
 * Full device-code OAuth flow. Resolves to an OAuthCredential on success.
 */
export async function runDeviceCodeFlow(
  provider: string,
  onCode: (verificationUri: string, userCode: string) => void,
): Promise<OAuthCredential> {
  const config = getOAuthConfig(provider);
  if (!config) throw new Error(`OAuth not supported for provider: ${provider}`);

  const deviceCode = await requestDeviceCode(config);
  onCode(deviceCode.verificationUriComplete ?? deviceCode.verificationUri, deviceCode.userCode);

  const token = await pollForToken(
    config,
    deviceCode.deviceCode,
    deviceCode.interval * 1000,
    deviceCode.expiresIn * 1000,
  );

  return {
    type: "oauth",
    access: token.accessToken,
    refresh: token.refreshToken ?? "",
    expires: token.expiresIn ? Date.now() + token.expiresIn * 1000 : Date.now() + 3600_000,
  };
}
