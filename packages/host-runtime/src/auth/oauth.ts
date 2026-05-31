/**
 * OAuth helpers — device-code flow for provider authentication.
 *
 * Implements RFC 8628 device authorization grant with:
 * - AbortSignal support for cancellation
 * - Proper slow_down interval handling (RFC 8628 §3.5)
 * - Pi-compatible poller abstraction
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
  deviceAuthorizationEndpoint: string;
  tokenEndpoint: string;
  clientId: string;
  scopes?: string[];
}

// ============================================================================
// Polling state machine (RFC 8628)
// ============================================================================

type PollResult =
  | { status: "complete"; value: OAuthTokenResponse }
  | { status: "pending" }
  | { status: "slow_down" }
  | { status: "failed"; message: string };

/** RFC 8628 §3.2: minimum polling interval is 1 second. */
const MINIMUM_INTERVAL_MS = 1000;
/** RFC 8628 §3.2: default interval when the server omits it. */
const DEFAULT_POLL_INTERVAL_SECONDS = 5;
/** RFC 8628 §3.5: slow_down means increase interval by 5 seconds. */
const SLOW_DOWN_INTERVAL_INCREMENT_MS = 5000;

const CANCEL_MESSAGE = "Login cancelled";
const TIMEOUT_MESSAGE = "Device flow timed out";
const SLOW_DOWN_TIMEOUT_MESSAGE =
  "Device flow timed out after one or more slow_down responses. " +
  "This is often caused by clock drift in WSL or VM environments. " +
  "Please sync or restart the VM clock and try again.";

function abortableSleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new Error(CANCEL_MESSAGE));
      return;
    }
    const onAbort = () => {
      clearTimeout(timeout);
      reject(new Error(CANCEL_MESSAGE));
    };
    const timeout = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

/**
 * Generic RFC 8628 device-code poller.
 * Calls `poll()` at the specified interval until it returns `complete`
 * or the deadline expires.
 */
export async function pollOAuthDeviceCodeFlow(
  poll: () => Promise<PollResult>,
  options: {
    intervalSeconds?: number;
    expiresInSeconds?: number;
    signal?: AbortSignal;
  } = {},
): Promise<OAuthTokenResponse> {
  const deadline =
    typeof options.expiresInSeconds === "number"
      ? Date.now() + options.expiresInSeconds * 1000
      : Number.POSITIVE_INFINITY;
  let intervalMs = Math.max(
    MINIMUM_INTERVAL_MS,
    Math.floor((options.intervalSeconds ?? DEFAULT_POLL_INTERVAL_SECONDS) * 1000),
  );

  let slowDownResponses = 0;
  while (Date.now() < deadline) {
    if (options.signal?.aborted) {
      throw new Error(CANCEL_MESSAGE);
    }

    const result = await poll();
    if (result.status === "complete") return result.value;
    if (result.status === "failed") throw new Error(result.message);
    if (result.status === "slow_down") {
      slowDownResponses += 1;
      // RFC 8628 §3.5: apply this increase to all subsequent requests
      intervalMs = Math.max(MINIMUM_INTERVAL_MS, intervalMs + SLOW_DOWN_INTERVAL_INCREMENT_MS);
    }

    const remainingMs = deadline - Date.now();
    if (remainingMs <= 0) break;

    await abortableSleep(Math.min(intervalMs, remainingMs), options.signal);
  }

  throw new Error(slowDownResponses > 0 ? SLOW_DOWN_TIMEOUT_MESSAGE : TIMEOUT_MESSAGE);
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
// Device code request
// ============================================================================

/** Step 1: Request a device code from the provider. */
export async function requestDeviceCode(
  config: OAuthProviderConfig,
  signal?: AbortSignal,
): Promise<OAuthDeviceCodeResponse> {
  const body = new URLSearchParams({
    client_id: config.clientId,
    scope: (config.scopes ?? []).join(" "),
  });

  const response = await fetch(config.deviceAuthorizationEndpoint, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: body.toString(),
    signal,
  });

  if (!response.ok) {
    throw new Error(`OAuth device authorization failed: ${response.status} ${response.statusText}`);
  }

  return response.json() as Promise<OAuthDeviceCodeResponse>;
}

// ============================================================================
// Token polling
// ============================================================================

/**
 * Step 2: Poll for token using RFC 8628 compliant polling.
 */
export async function pollForToken(
  config: OAuthProviderConfig,
  deviceCode: string,
  options: { intervalSeconds?: number; expiresInSeconds?: number; signal?: AbortSignal } = {},
): Promise<OAuthTokenResponse> {
  return pollOAuthDeviceCodeFlow(
    async () => {
      const body = new URLSearchParams({
        grant_type: "urn:ietf:params:oauth:grant-type:device_code",
        device_code: deviceCode,
        client_id: config.clientId,
      });

      const response = await fetch(config.tokenEndpoint, {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: body.toString(),
        signal: options.signal,
      });

      if (response.ok) {
        const value = (await response.json()) as OAuthTokenResponse;
        return { status: "complete" as const, value };
      }

      const data = (await response.json().catch(() => ({}))) as {
        error?: string;
        error_description?: string;
      };
      if (data.error === "slow_down") return { status: "slow_down" as const };
      if (data.error === "authorization_pending") return { status: "pending" as const };

      return {
        status: "failed" as const,
        message: data.error_description ?? data.error ?? `HTTP ${response.status}`,
      };
    },
    {
      intervalSeconds: options.intervalSeconds,
      expiresInSeconds: options.expiresInSeconds,
      signal: options.signal,
    },
  );
}

// ============================================================================
// Full flow
// ============================================================================

/**
 * Full device-code OAuth flow. Resolves to an OAuthCredential on success.
 * Accepts an optional AbortSignal for cancellation.
 */
export async function runDeviceCodeFlow(
  provider: string,
  onCode: (verificationUri: string, userCode: string) => void,
  signal?: AbortSignal,
): Promise<OAuthCredential> {
  const config = getOAuthConfig(provider);
  if (!config) throw new Error(`OAuth not supported for provider: ${provider}`);

  const deviceCode = await requestDeviceCode(config, signal);
  onCode(deviceCode.verificationUriComplete ?? deviceCode.verificationUri, deviceCode.userCode);

  const token = await pollForToken(config, deviceCode.deviceCode, {
    intervalSeconds: deviceCode.interval,
    expiresInSeconds: deviceCode.expiresIn,
    signal,
  });

  return {
    type: "oauth",
    access: token.accessToken,
    refresh: token.refreshToken ?? "",
    expires: token.expiresIn ? Date.now() + token.expiresIn * 1000 : Date.now() + 3600_000,
  };
}
