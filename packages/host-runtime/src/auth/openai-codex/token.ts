import type { OAuthCredentials } from "../oauth-types.js";
import {
  CLIENT_ID,
  JWT_CLAIM_PATH,
  type JwtPayload,
  type OAuthToken,
  REDIRECT_URI,
  TOKEN_URL,
  type TokenOperation,
} from "./constants.js";

export async function fetchWithLoginCancellation(
  input: string,
  init: RequestInit,
): Promise<Response> {
  try {
    return await fetch(input, init);
  } catch (error) {
    if (init.signal?.aborted) {
      throw new Error("Login cancelled");
    }
    throw error;
  }
}

async function readTokenResponse(
  response: Response,
  operation: TokenOperation,
): Promise<OAuthToken> {
  if (!response.ok) {
    const text = await response.text().catch(() => "");
    throw new Error(
      `OpenAI Codex token ${operation} failed (${response.status}): ${text || response.statusText}`,
    );
  }

  const rawJson = await response.json();
  const json = rawJson as {
    access_token?: string;
    refresh_token?: string;
    expires_in?: number;
  } | null;
  if (!json?.access_token || !json.refresh_token || typeof json.expires_in !== "number") {
    throw new Error(
      `OpenAI Codex token ${operation} response missing fields: ${JSON.stringify(json)}`,
    );
  }

  return {
    access: json.access_token,
    refresh: json.refresh_token,
    expires: Date.now() + json.expires_in * 1000,
  };
}

export async function exchangeAuthorizationCode(
  code: string,
  verifier: string,
  redirectUri: string = REDIRECT_URI,
  signal?: AbortSignal,
): Promise<OAuthToken> {
  const response = await fetchWithLoginCancellation(TOKEN_URL, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      grant_type: "authorization_code",
      client_id: CLIENT_ID,
      code,
      code_verifier: verifier,
      redirect_uri: redirectUri,
    }),
    signal,
  });

  return readTokenResponse(response, "exchange");
}

async function refreshAccessToken(refreshToken: string): Promise<OAuthToken> {
  let response: Response;
  try {
    response = await fetch(TOKEN_URL, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        grant_type: "refresh_token",
        refresh_token: refreshToken,
        client_id: CLIENT_ID,
      }),
    });
  } catch (error) {
    throw new Error(
      `OpenAI Codex token refresh error: ${error instanceof Error ? error.message : String(error)}`,
    );
  }

  return readTokenResponse(response, "refresh");
}

function decodeJwt(token: string): JwtPayload | null {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) return null;
    const payload = parts[1] ?? "";
    const decoded = atob(payload);
    return JSON.parse(decoded) as JwtPayload;
  } catch {
    return null;
  }
}

function getAccountId(accessToken: string): string | null {
  const payload = decodeJwt(accessToken);
  const auth = payload?.[JWT_CLAIM_PATH];
  const accountId = auth?.chatgpt_account_id;
  return typeof accountId === "string" && accountId.length > 0 ? accountId : null;
}

function credentialsFromToken(token: OAuthToken): OAuthCredentials {
  const accountId = getAccountId(token.access);
  if (!accountId) {
    throw new Error("Failed to extract accountId from token");
  }

  return {
    access: token.access,
    refresh: token.refresh,
    expires: token.expires,
    accountId,
  };
}

export async function exchangeAuthorizationCodeForCredentials(
  code: string,
  verifier: string,
  redirectUri: string,
  signal?: AbortSignal,
): Promise<OAuthCredentials> {
  return credentialsFromToken(await exchangeAuthorizationCode(code, verifier, redirectUri, signal));
}

export async function refreshOpenAICodexToken(refreshToken: string): Promise<OAuthCredentials> {
  return credentialsFromToken(await refreshAccessToken(refreshToken));
}
