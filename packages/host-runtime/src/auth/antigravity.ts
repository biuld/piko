/**
 * Antigravity (Google Cloud Code) OAuth flow.
 *
 * Reimplements the @raquezha/noagy OAuth logic as a built-in piko provider.
 * Uses PKCE + local callback server + Google OAuth token exchange.
 * Discovers the project ID after login via the Antigravity API.
 */

import { oauthErrorHtml, oauthSuccessHtml } from "./oauth-page.js";
import type {
  OAuthCredentials,
  OAuthLoginCallbacks,
  OAuthProviderInterface,
} from "./oauth-types.js";
import { generatePKCE } from "./pkce.js";

// ============================================================================
// Constants
// ============================================================================

const decode = (s: string) => atob(s);

const CLIENT_ID =
  process.env.ANTIGRAVITY_CLIENT_ID ||
  process.env.NOAGY_CLIENT_ID ||
  decode(
    "MTA3MTAwNjA2MDU5MS10bWhzc2luMmgyMWxjcmUyMzV2dG9sb2poNGc0MDNlc" +
      "C5hcHBzLmdvb2dsZXVzZXJjb250ZW50LmNvbQ==",
  );

const CLIENT_SECRET =
  process.env.ANTIGRAVITY_CLIENT_SECRET ||
  process.env.NOAGY_CLIENT_SECRET ||
  decode("R09DU1BYLUs1OEZXUjQ" + "4NkxkTEoxbUxCOHNYQzR6NnFEQWY=");

const SCOPES = [
  "https://www.googleapis.com/auth/cloud-platform",
  "https://www.googleapis.com/auth/userinfo.email",
  "https://www.googleapis.com/auth/userinfo.profile",
  "https://www.googleapis.com/auth/cclog",
  "https://www.googleapis.com/auth/experimentsandconfigs",
];

const AUTH_URL = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL = "https://oauth2.googleapis.com/token";
const DEFAULT_ENDPOINT = "https://daily-cloudcode-pa.googleapis.com";
const REDIRECT_URI = "http://localhost:51121/oauth-callback";
const CALLBACK_HOST =
  process.env.ANTIGRAVITY_CALLBACK_HOST || process.env.NOAGY_CALLBACK_HOST || "127.0.0.1";
const CALLBACK_PORT = 51121;
const CALLBACK_PATH = "/oauth-callback";

// ============================================================================
// Helpers
// ============================================================================

function antigravityEnv(name: string): string | undefined {
  return process.env[`ANTIGRAVITY_${name}`] || process.env[`NOAGY_${name}`];
}

function endpointCandidates(): string[] {
  const explicit = antigravityEnv("BASE_URL")?.trim();
  return explicit ? [explicit] : [DEFAULT_ENDPOINT];
}

/**
 * Generate a stable UUID (v5-like) from a seed string using SHA-1.
 * This mirrors noagy's stableProjectId().
 */
async function stableProjectId(seed: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(`antigravity:${seed}`);
  const hashBuffer = await crypto.subtle.digest("SHA-1", data);
  const bytes = new Uint8Array(hashBuffer).subarray(0, 16);

  // Set UUID version (5) and variant bits
  bytes[6] = (bytes[6] & 0x0f) | 0x50;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;

  const hex = Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function antigravityHeaders(token: string): Record<string, string> {
  return {
    Authorization: `Bearer ${token}`,
    "Content-Type": "application/json",
    Accept: "text/event-stream",
    "User-Agent": antigravityEnv("USER_AGENT") || "antigravity/1.0.5 darwin/arm64",
    "X-Goog-Api-Client": "google-api-nodejs-client/9.15.1",
    "Client-Metadata": JSON.stringify({ ideType: "ANTIGRAVITY" }),
  };
}

// ============================================================================
// Project discovery
// ============================================================================

function extractProjectId(data: unknown): string | undefined {
  if (!data || typeof data !== "object") return undefined;

  const obj = data as Record<string, unknown>;
  const direct =
    obj.antigravityProjectId ??
    obj.projectId ??
    obj.backendProjectId ??
    obj.userDefinedCloudaicompanionProject ??
    obj.cloudaicompanionProject ??
    obj.project;

  if (typeof direct === "string" && direct) return direct;
  if (
    direct &&
    typeof direct === "object" &&
    typeof (direct as Record<string, unknown>).id === "string"
  ) {
    return (direct as Record<string, unknown>).id as string;
  }

  // Search nested arrays
  for (const key of ["projects", "projectIds", "cloudaicompanionProjects"]) {
    const value = obj[key];
    if (Array.isArray(value)) {
      for (const item of value) {
        const nested = extractProjectId(item);
        if (nested) return nested;
        if (typeof item === "string" && item) return item;
      }
    }
  }
  return undefined;
}

async function listCloudAICompanionProjects(token: string): Promise<string | undefined> {
  for (const endpoint of endpointCandidates()) {
    try {
      const res = await fetch(`${endpoint}/v1internal:listCloudAICompanionProjects`, {
        method: "POST",
        headers: antigravityHeaders(token),
        body: JSON.stringify({}),
      });
      if (!res.ok) continue;
      return extractProjectId(await res.json());
    } catch {
      // continue
    }
  }
  return undefined;
}

async function loadCodeAssist(token: string): Promise<string | undefined> {
  const body = JSON.stringify({ metadata: { ideType: "ANTIGRAVITY" } });

  for (const endpoint of endpointCandidates()) {
    try {
      const res = await fetch(`${endpoint}/v1internal:loadCodeAssist`, {
        method: "POST",
        headers: antigravityHeaders(token),
        body,
      });
      if (!res.ok) continue;
      const project = extractProjectId(await res.json());
      if (project) return project;
      return await listCloudAICompanionProjects(token);
    } catch {
      // continue
    }
  }
  return undefined;
}

// ============================================================================
// OAuth helpers
// ============================================================================

async function getUserEmail(token: string): Promise<string | undefined> {
  try {
    const res = await fetch("https://www.googleapis.com/oauth2/v1/userinfo?alt=json", {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) return undefined;
    const data = (await res.json()) as { email?: string };
    return data.email;
  } catch {
    return undefined;
  }
}

type CallbackResult = { code: string; state: string };

function startCallbackServer(expectedState: string): Promise<{
  server: ReturnType<typeof Bun.serve>;
  waitForCode: () => Promise<CallbackResult>;
}> {
  return new Promise((resolve, reject) => {
    let settleWait: ((value: CallbackResult) => void) | undefined;
    const waitForCodePromise = new Promise<CallbackResult>((resolveWait) => {
      settleWait = resolveWait;
    });

    try {
      const server = Bun.serve({
        hostname: CALLBACK_HOST,
        port: CALLBACK_PORT,
        fetch(req) {
          try {
            const url = new URL(req.url);
            if (url.pathname !== CALLBACK_PATH) {
              return new Response(oauthErrorHtml("Callback route not found."), {
                status: 404,
                headers: { "Content-Type": "text/html; charset=utf-8" },
              });
            }

            const error = url.searchParams.get("error");
            const code = url.searchParams.get("code");
            const state = url.searchParams.get("state");

            if (error) {
              return new Response(
                oauthErrorHtml("Antigravity authentication did not complete.", `Error: ${error}`),
                {
                  status: 400,
                  headers: { "Content-Type": "text/html; charset=utf-8" },
                },
              );
            }

            if (!code || !state) {
              return new Response(oauthErrorHtml("Missing code or state parameter."), {
                status: 400,
                headers: { "Content-Type": "text/html; charset=utf-8" },
              });
            }

            if (state !== expectedState) {
              return new Response(oauthErrorHtml("State mismatch."), {
                status: 400,
                headers: { "Content-Type": "text/html; charset=utf-8" },
              });
            }

            settleWait?.({ code, state });
            return new Response(
              oauthSuccessHtml(
                "Antigravity authentication completed. You can close this window and return to piko.",
              ),
              {
                status: 200,
                headers: { "Content-Type": "text/html; charset=utf-8" },
              },
            );
          } catch {
            return new Response("Internal error", {
              status: 500,
              headers: { "Content-Type": "text/plain; charset=utf-8" },
            });
          }
        },
      });

      resolve({ server, waitForCode: () => waitForCodePromise });
    } catch (error) {
      reject(error);
    }
  });
}

async function exchangeToken(
  params: Record<string, string>,
): Promise<{ access_token: string; refresh_token: string; expires_in: number }> {
  const response = await fetch(TOKEN_URL, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams(params).toString(),
    signal: AbortSignal.timeout(30_000),
  });

  const responseBody = await response.text();
  if (!response.ok) {
    throw new Error(`Token exchange failed: ${response.status} ${responseBody}`);
  }

  try {
    return JSON.parse(responseBody) as {
      access_token: string;
      refresh_token: string;
      expires_in: number;
    };
  } catch {
    throw new Error(`Token exchange returned invalid JSON: ${responseBody}`);
  }
}

// ============================================================================
// Antigravity OAuth credentials (extends base with projectId)
// ============================================================================

export type AntigravityCredentials = OAuthCredentials & {
  projectId: string;
  email?: string;
};

// ============================================================================
// Public API: login, refresh, getApiKey
// ============================================================================

export async function loginAntigravity(
  callbacks: OAuthLoginCallbacks,
): Promise<AntigravityCredentials> {
  const { verifier, challenge } = await generatePKCE();
  const { server, waitForCode } = await startCallbackServer(verifier);

  try {
    const authParams = new URLSearchParams({
      client_id: CLIENT_ID,
      response_type: "code",
      redirect_uri: REDIRECT_URI,
      scope: SCOPES.join(" "),
      code_challenge: challenge,
      code_challenge_method: "S256",
      state: verifier,
      access_type: "offline",
      prompt: "consent",
    });

    callbacks.onAuth({
      url: `${AUTH_URL}?${authParams.toString()}`,
      instructions: "Complete Google sign-in. piko will capture the local callback.",
    });

    const { code, state } = await waitForCode();

    if (state !== verifier) {
      throw new Error("OAuth state mismatch");
    }

    callbacks.onProgress?.("Exchanging authorization code for tokens...");
    const tokenData = await exchangeToken({
      client_id: CLIENT_ID,
      client_secret: CLIENT_SECRET,
      code,
      grant_type: "authorization_code",
      redirect_uri: REDIRECT_URI,
      code_verifier: verifier,
    });

    if (!tokenData.refresh_token) {
      throw new Error("No refresh token received. Re-run /login and allow offline access.");
    }

    callbacks.onProgress?.("Discovering project...");
    const [email, discoveredProject] = await Promise.all([
      getUserEmail(tokenData.access_token),
      loadCodeAssist(tokenData.access_token),
    ]);

    const defaultProjectId =
      antigravityEnv("PROJECT_ID")?.trim() || (await stableProjectId(process.cwd()));

    return {
      refresh: tokenData.refresh_token,
      access: tokenData.access_token,
      expires: Date.now() + tokenData.expires_in * 1000 - 5 * 60 * 1000,
      projectId: discoveredProject || defaultProjectId,
      email,
    };
  } finally {
    server.stop();
  }
}

export async function refreshAntigravityToken(
  credentials: AntigravityCredentials,
): Promise<AntigravityCredentials> {
  const tokenData = await exchangeToken({
    client_id: CLIENT_ID,
    client_secret: CLIENT_SECRET,
    refresh_token: credentials.refresh,
    grant_type: "refresh_token",
  });

  const discoveredProject = await loadCodeAssist(tokenData.access_token);
  const defaultProjectId =
    antigravityEnv("PROJECT_ID")?.trim() || (await stableProjectId(process.cwd()));

  return {
    ...credentials,
    refresh: tokenData.refresh_token || credentials.refresh,
    access: tokenData.access_token,
    expires: Date.now() + tokenData.expires_in * 1000 - 5 * 60 * 1000,
    projectId: discoveredProject || credentials.projectId || defaultProjectId,
  };
}

export function getAntigravityApiKey(credentials: AntigravityCredentials): string {
  return JSON.stringify({ token: credentials.access, projectId: credentials.projectId });
}

// ============================================================================
// OAuthProviderInterface
// ============================================================================

export const antigravityOAuthProvider: OAuthProviderInterface = {
  id: "antigravity",
  name: "Antigravity (Google Cloud Code)",
  usesCallbackServer: true,

  async login(callbacks: OAuthLoginCallbacks): Promise<OAuthCredentials> {
    return loginAntigravity(callbacks);
  },

  async refreshToken(credentials: OAuthCredentials): Promise<OAuthCredentials> {
    return refreshAntigravityToken(credentials as AntigravityCredentials);
  },

  getApiKey(credentials: OAuthCredentials): string {
    return getAntigravityApiKey(credentials as AntigravityCredentials);
  },
};
