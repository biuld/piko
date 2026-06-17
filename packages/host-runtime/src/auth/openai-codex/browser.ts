import { oauthErrorHtml, oauthSuccessHtml } from "../oauth-page.js";
import type { OAuthCredentials, OAuthPrompt } from "../oauth-types.js";
import { generatePKCE } from "../pkce.js";
import { AUTHORIZE_URL, CALLBACK_HOST, CLIENT_ID, REDIRECT_URI, SCOPE } from "./constants.js";
import { exchangeAuthorizationCodeForCredentials } from "./token.js";

function createState(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function parseAuthorizationInput(input: string): { code?: string; state?: string } {
  const value = input.trim();
  if (!value) return {};

  try {
    const url = new URL(value);
    return {
      code: url.searchParams.get("code") ?? undefined,
      state: url.searchParams.get("state") ?? undefined,
    };
  } catch {}

  if (value.includes("#")) {
    const [code, state] = value.split("#", 2);
    return { code, state };
  }

  if (value.includes("code=")) {
    const params = new URLSearchParams(value);
    return {
      code: params.get("code") ?? undefined,
      state: params.get("state") ?? undefined,
    };
  }

  return { code: value };
}

async function createAuthorizationFlow(
  originator: string = "pi",
): Promise<{ verifier: string; state: string; url: string }> {
  const { verifier, challenge } = await generatePKCE();
  const state = createState();

  const url = new URL(AUTHORIZE_URL);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("client_id", CLIENT_ID);
  url.searchParams.set("redirect_uri", REDIRECT_URI);
  url.searchParams.set("scope", SCOPE);
  url.searchParams.set("code_challenge", challenge);
  url.searchParams.set("code_challenge_method", "S256");
  url.searchParams.set("state", state);
  url.searchParams.set("id_token_add_organizations", "true");
  url.searchParams.set("codex_cli_simplified_flow", "true");
  url.searchParams.set("originator", originator);

  return { verifier, state, url: url.toString() };
}

type OAuthServerInfo = {
  close: () => void;
  cancelWait: () => void;
  waitForCode: () => Promise<{ code: string } | null>;
};

function startLocalOAuthServer(state: string): Promise<OAuthServerInfo> {
  let settleWait: ((value: { code: string } | null) => void) | undefined;
  const waitForCodePromise = new Promise<{ code: string } | null>((resolve) => {
    let settled = false;
    settleWait = (value) => {
      if (settled) return;
      settled = true;
      resolve(value);
    };
  });

  return new Promise((resolve) => {
    try {
      const server = Bun.serve({
        hostname: CALLBACK_HOST,
        port: 1455,
        fetch(req) {
          try {
            const url = new URL(req.url);
            if (url.pathname !== "/auth/callback") {
              return new Response(oauthErrorHtml("Callback route not found."), {
                status: 404,
                headers: { "Content-Type": "text/html; charset=utf-8" },
              });
            }
            if (url.searchParams.get("state") !== state) {
              return new Response(oauthErrorHtml("State mismatch."), {
                status: 400,
                headers: { "Content-Type": "text/html; charset=utf-8" },
              });
            }
            const code = url.searchParams.get("code");
            if (!code) {
              return new Response(oauthErrorHtml("Missing authorization code."), {
                status: 400,
                headers: { "Content-Type": "text/html; charset=utf-8" },
              });
            }
            settleWait?.({ code });
            return new Response(
              oauthSuccessHtml("OpenAI authentication completed. You can close this window."),
              {
                status: 200,
                headers: { "Content-Type": "text/html; charset=utf-8" },
              },
            );
          } catch {
            return new Response(oauthErrorHtml("Internal error while processing OAuth callback."), {
              status: 500,
              headers: { "Content-Type": "text/html; charset=utf-8" },
            });
          }
        },
      });

      resolve({
        close: () => server.stop(),
        cancelWait: () => {
          settleWait?.(null);
        },
        waitForCode: () => waitForCodePromise,
      });
    } catch {
      settleWait?.(null);
      resolve({
        close: () => {},
        cancelWait: () => {},
        waitForCode: async () => null,
      });
    }
  });
}

export async function loginOpenAICodex(options: {
  onAuth: (info: { url: string; instructions?: string }) => void;
  onPrompt: (prompt: OAuthPrompt) => Promise<string>;
  onProgress?: (message: string) => void;
  onManualCodeInput?: () => Promise<string>;
  originator?: string;
}): Promise<OAuthCredentials> {
  const { verifier, state, url } = await createAuthorizationFlow(options.originator);
  const server = await startLocalOAuthServer(state);

  options.onAuth({ url, instructions: "A browser window should open. Complete login to finish." });

  let code: string | undefined;
  try {
    if (options.onManualCodeInput) {
      let manualCode: string | undefined;
      let manualError: Error | undefined;
      const manualPromise = options
        .onManualCodeInput()
        .then((input) => {
          manualCode = input;
          server.cancelWait();
        })
        .catch((err) => {
          manualError = err instanceof Error ? err : new Error(String(err));
          server.cancelWait();
        });

      const result = await server.waitForCode();
      if (manualError) {
        throw manualError;
      }

      if (result?.code) {
        code = result.code;
      } else if (manualCode) {
        const parsed = parseAuthorizationInput(manualCode);
        if (parsed.state && parsed.state !== state) {
          throw new Error("State mismatch");
        }
        code = parsed.code;
      }

      if (!code) {
        await manualPromise;
        if (manualError) {
          throw manualError;
        }
        if (manualCode) {
          const parsed = parseAuthorizationInput(manualCode);
          if (parsed.state && parsed.state !== state) {
            throw new Error("State mismatch");
          }
          code = parsed.code;
        }
      }
    } else {
      const result = await server.waitForCode();
      if (result?.code) {
        code = result.code;
      }
    }

    if (!code) {
      const input = await options.onPrompt({
        message: "Paste the authorization code (or full redirect URL):",
      });
      const parsed = parseAuthorizationInput(input);
      if (parsed.state && parsed.state !== state) {
        throw new Error("State mismatch");
      }
      code = parsed.code;
    }

    if (!code) {
      throw new Error("Missing authorization code");
    }

    return exchangeAuthorizationCodeForCredentials(code, verifier, REDIRECT_URI);
  } finally {
    server.close();
  }
}
