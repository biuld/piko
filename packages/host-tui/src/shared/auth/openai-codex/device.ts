import { pollOAuthDeviceCodeFlow } from "../device-code.js";
import type { OAuthCredentials, OAuthDeviceCodeInfo } from "../oauth-types.js";
import {
  CLIENT_ID,
  DEVICE_CODE_TIMEOUT_SECONDS,
  DEVICE_REDIRECT_URI,
  DEVICE_TOKEN_URL,
  DEVICE_USER_CODE_URL,
  DEVICE_VERIFICATION_URI,
  type DeviceAuthInfo,
  type DeviceTokenSuccess,
} from "./constants.js";
import { exchangeAuthorizationCodeForCredentials, fetchWithLoginCancellation } from "./token.js";

async function startOpenAICodexDeviceAuth(signal?: AbortSignal): Promise<DeviceAuthInfo> {
  const response = await fetchWithLoginCancellation(DEVICE_USER_CODE_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ client_id: CLIENT_ID }),
    signal,
  });

  if (!response.ok) {
    if (response.status === 404) {
      throw new Error(
        "OpenAI Codex device code login is not enabled for this server. Use browser login or verify the server URL.",
      );
    }
    const responseBody = await response.text().catch(() => "");
    throw new Error(
      `OpenAI Codex device code request failed with status ${response.status}${responseBody ? `: ${responseBody}` : ""}`,
    );
  }

  const rawJson = await response.json();
  const json = rawJson as {
    device_auth_id?: string;
    user_code?: string;
    interval?: number | string;
  } | null;
  const intervalSeconds =
    typeof json?.interval === "string" ? Number(json.interval.trim()) : json?.interval;
  if (
    !json?.device_auth_id ||
    !json.user_code ||
    typeof intervalSeconds !== "number" ||
    !Number.isFinite(intervalSeconds) ||
    intervalSeconds < 0
  ) {
    throw new Error(`Invalid OpenAI Codex device code response: ${JSON.stringify(json)}`);
  }

  return {
    deviceAuthId: json.device_auth_id,
    userCode: json.user_code,
    intervalSeconds,
  };
}

async function pollOpenAICodexDeviceAuth(
  device: DeviceAuthInfo,
  signal?: AbortSignal,
): Promise<DeviceTokenSuccess> {
  return pollOAuthDeviceCodeFlow<DeviceTokenSuccess>({
    intervalSeconds: device.intervalSeconds,
    expiresInSeconds: DEVICE_CODE_TIMEOUT_SECONDS,
    signal,
    poll: async () => {
      const response = await fetchWithLoginCancellation(DEVICE_TOKEN_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          device_auth_id: device.deviceAuthId,
          user_code: device.userCode,
        }),
        signal,
      });

      if (response.ok) {
        const rawJson = await response.json();
        const json = rawJson as { authorization_code?: string; code_verifier?: string } | null;
        if (!json?.authorization_code || !json.code_verifier) {
          return {
            status: "failed",
            message: `Invalid OpenAI Codex device auth token response: ${JSON.stringify(json)}`,
          };
        }
        return {
          status: "complete",
          value: { authorizationCode: json.authorization_code, codeVerifier: json.code_verifier },
        };
      }

      if (response.status === 403 || response.status === 404) {
        return { status: "pending" };
      }

      const responseBody = await response.text().catch(() => "");
      let errorCode: unknown;
      try {
        const json = JSON.parse(responseBody) as { error?: string | { code?: string } } | null;
        const error = json?.error;
        errorCode = typeof error === "object" ? error?.code : error;
      } catch {}

      if (errorCode === "deviceauth_authorization_pending") {
        return { status: "pending" };
      }
      if (errorCode === "slow_down") {
        return { status: "slow_down" };
      }

      return {
        status: "failed",
        message: `OpenAI Codex device auth failed with status ${response.status}${responseBody ? `: ${responseBody}` : ""}`,
      };
    },
  });
}

export async function loginOpenAICodexDeviceCode(options: {
  onDeviceCode: (info: OAuthDeviceCodeInfo) => void;
  signal?: AbortSignal;
}): Promise<OAuthCredentials> {
  const device = await startOpenAICodexDeviceAuth(options.signal);
  options.onDeviceCode({
    userCode: device.userCode,
    verificationUri: DEVICE_VERIFICATION_URI,
    intervalSeconds: device.intervalSeconds,
    expiresInSeconds: DEVICE_CODE_TIMEOUT_SECONDS,
  });
  const code = await pollOpenAICodexDeviceAuth(device, options.signal);
  return exchangeAuthorizationCodeForCredentials(
    code.authorizationCode,
    code.codeVerifier,
    DEVICE_REDIRECT_URI,
    options.signal,
  );
}
