/**
 * Login Flow — multi-step provider authentication orchestrator.
 *
 * Flow:
 *   1. Auth type selector: "Use a subscription" (OAuth) vs "Use an API key"
 *   2. Provider list for chosen auth type
 *   3. API key input or OAuth flow via authStorage.login()
 */

import {
  Container,
  type Focusable,
  type SelectItem,
  SelectList,
  Spacer,
  Text,
} from "@earendil-works/pi-tui";
import { AuthStorage, getOAuthProviders, listAvailableModels } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import type { OverlayContext } from "../overlays/index.js";
import type { AuthType } from "../overlays/login-dialog.js";
import { LoginDialogComponent } from "../overlays/login-dialog.js";
import { getSelectListTheme, getTheme } from "../theme.js";

// ============================================================================
// Types
// ============================================================================

interface ProviderOption {
  id: string;
  name: string;
  authType: AuthType;
  configured: boolean;
}

// ============================================================================
// Selector helpers
// ============================================================================

/** Build list of provider options with auth status */
function buildProviderOptions(authStorage: AuthStorage, authType: AuthType): ProviderOption[] {
  const results: ProviderOption[] = [];

  // OAuth providers from the registry (Anthropic, OpenAI Codex, GitHub Copilot)
  const oauthProviderIds = new Set(getOAuthProviders().map((p) => p.id));

  // Get all available model providers
  const modelProviders = listAvailableModels();

  const seen = new Set<string>();
  for (const p of modelProviders) {
    if (seen.has(p.provider)) continue;
    seen.add(p.provider);

    const hasOAuth = oauthProviderIds.has(p.provider.toLowerCase());
    const providerAuthType = hasOAuth ? "oauth" : "api_key";

    // Only include providers matching the requested auth type
    if (authType === "oauth" && !hasOAuth) continue;
    if (authType === "api_key" && hasOAuth) continue;

    const cred = authStorage.get(p.provider);
    const configured = !!cred;

    results.push({
      id: p.provider,
      name: p.provider.charAt(0).toUpperCase() + p.provider.slice(1),
      authType: providerAuthType,
      configured,
    });
  }

  // Also add OAuth-only providers that don't have models listed
  for (const oauthProvider of getOAuthProviders()) {
    if (!seen.has(oauthProvider.id)) {
      if (authType !== "oauth") continue;
      const cred = authStorage.get(oauthProvider.id);
      results.push({
        id: oauthProvider.id,
        name: oauthProvider.name,
        authType: "oauth",
        configured: !!cred,
      });
    }
  }

  return results;
}

/** Show a simple SelectList and return the selected value */
function showSelectList(
  ctx: OverlayContext,
  title: string,
  items: SelectItem[],
): Promise<string | undefined> {
  return new Promise((resolve) => {
    const t = getTheme();
    const borderColor = (s: string) => t.fg("border", s);

    const selectList = new SelectList(items, Math.min(items.length, 10), getSelectListTheme());

    const container = new Container();
    container.addChild(new DynamicBorder(borderColor));
    container.addChild(new Text(t.fg("accent", t.bold(` ${title}`)), 1, 0));
    container.addChild(new Spacer(1));
    container.addChild(selectList);
    container.addChild(new Spacer(1));
    container.addChild(
      new Text(
        `${keyHint("tui.select.confirm", "select")}  ${keyHint("tui.select.cancel", "back")}`,
        1,
        0,
      ),
    );
    container.addChild(new DynamicBorder(borderColor));

    selectList.onSelect = (item) => {
      replacementHandle?.hide();
      resolve(item.value);
    };
    selectList.onCancel = () => {
      replacementHandle?.hide();
      resolve(undefined);
    };

    // Simple focusable container
    let _focused = false;
    Object.defineProperty(container, "focused", {
      get() {
        return _focused;
      },
      set(v: boolean) {
        _focused = v;
      },
      enumerable: true,
      configurable: true,
    });
    Object.assign(container, {
      handleInput(data: string) {
        selectList.handleInput(data);
        ctx.tui.requestRender();
      },
    });

    const replacementHandle = ctx.showReplacement(container as Container & Focusable);
  });
}

// ============================================================================
// API Key flow
// ============================================================================

async function openApiKeyInDialog(
  ctx: OverlayContext,
  providerId: string,
  providerName: string,
  authStorage: AuthStorage,
): Promise<boolean> {
  return new Promise((resolve) => {
    const dialog = new LoginDialogComponent(
      ctx.tui,
      providerId,
      providerName,
      "api_key",
      (success) => {
        restoreEditor();
        resolve(success);
      },
    );

    const restoreEditor = () => {
      ctx.restoreEditor();
    };

    ctx.setActiveOverlay(ctx.showReplacement(dialog as any, dialog as any));

    // Show prompt for API key
    const existingCred = authStorage.get(providerId);

    dialog
      .showPrompt(
        existingCred?.type === "api_key"
          ? `Enter API key (current: ${"•".repeat(8)}):`
          : "Enter API key:",
        "sk-...",
      )
      .then((apiKey) => {
        const key = apiKey.trim();
        if (!key) {
          dialog.showInfo(["API key cannot be empty."]);
          return new Promise(() => {}); // Keep dialog open
        }
        authStorage.set(providerId, { type: "api_key", key });
        restoreEditor();
        ctx.msg("system", `✓ API key saved for ${providerName}.`);
        ctx.render();
        resolve(true);
      })
      .catch(() => {
        restoreEditor();
        resolve(false);
      });
  });
}

// ============================================================================
// OAuth flow
// ============================================================================

async function oauthShowSelect(
  dialog: LoginDialogComponent,
  message: string,
  options: { id: string; label: string }[],
): Promise<string | undefined> {
  // For select prompts, reuse dialog's info display + prompt
  const lines = [message, ""];
  for (const opt of options) {
    lines.push(`  ${opt.label}`);
  }
  const selected = await dialog.showPrompt(
    `Type selection (${options.map((o) => o.id).join("/")}):`,
    options[0]?.id,
  );
  const trimmed = selected.trim().toLowerCase();
  if (options.some((o) => o.id === trimmed)) return trimmed;
  return options[0]?.id; // default to first
}

async function openOAuthInDialog(
  ctx: OverlayContext,
  providerId: string,
  providerName: string,
  authStorage: AuthStorage,
): Promise<boolean> {
  return new Promise((resolve) => {
    const dialog = new LoginDialogComponent(
      ctx.tui,
      providerId,
      providerName,
      "oauth",
      (success) => {
        restoreEditor();
        resolve(success);
      },
    );

    const restoreEditor = () => {
      ctx.restoreEditor();
    };

    ctx.setActiveOverlay(ctx.showReplacement(dialog as any, dialog as any));

    // Use authStorage.login() which delegates to the appropriate OAuth provider
    void authStorage
      .login(providerId, {
        onAuth: (info) => {
          dialog.showAuth(info.url, info.instructions);
        },
        onDeviceCode: (info) => {
          dialog.showDeviceCode(info.verificationUri, info.userCode);
          dialog.showWaiting("Waiting for authentication...");
        },
        onPrompt: async (prompt) => {
          return dialog.showPrompt(prompt.message, prompt.placeholder);
        },
        onProgress: (message) => {
          dialog.showProgress(message);
        },
        onManualCodeInput: async () => {
          return dialog.showPrompt("Paste redirect URL below, or complete login in browser:");
        },
        onSelect: async (prompt) => {
          return (
            (await oauthShowSelect(dialog, prompt.message, prompt.options)) ?? prompt.options[0]?.id
          );
        },
        signal: dialog.signal,
      })
      .then(() => {
        const t = getTheme();
        dialog.showInfo([
          t.fg("success", "✓ OAuth authorized successfully"),
          "",
          t.fg("dim", "You can now close this dialog."),
        ]);
        ctx.msg("system", `✓ OAuth login successful for ${providerName}.`);
        ctx.render();
        resolve(true);
      })
      .catch((err: unknown) => {
        const msg = err instanceof Error ? err.message : String(err);
        if (msg.includes("cancelled") || msg.includes("cancel")) {
          restoreEditor();
          resolve(false);
          return;
        }
        const t = getTheme();
        dialog.showInfo([
          t.fg("error", `OAuth failed: ${msg}`),
          "",
          t.fg("dim", "Press Esc to close."),
        ]);
      });
  });
}

// ============================================================================
// Logout flow
// ============================================================================

/**
 * Open the logout flow — mirrors pi's `/logout` command.
 * Shows providers with stored credentials for removal.
 */
export async function openLogoutFlow(
  ctx: OverlayContext,
  authStorage: AuthStorage = AuthStorage.create(),
): Promise<boolean> {
  const t = getTheme();

  // Only show providers that have stored credentials
  const allProviders = buildProviderOptions(authStorage, "oauth").concat(
    buildProviderOptions(authStorage, "api_key"),
  );
  const configuredProviders = allProviders.filter((p) => p.configured);

  if (configuredProviders.length === 0) {
    ctx.msg(
      "system",
      "No stored credentials to remove. /logout only removes credentials saved by /login; environment variables and models.json config are unchanged.",
    );
    ctx.render();
    return false;
  }

  const providerItems: SelectItem[] = configuredProviders.map((p) => ({
    value: p.id,
    label: p.name,
    description: t.fg("success", `✓ ${p.authType === "oauth" ? "subscription" : "API key"}`),
  }));

  const providerId = await showSelectList(ctx, "Select provider to logout:", providerItems);

  if (!providerId) return false;

  const provider = configuredProviders.find((p) => p.id === providerId);
  if (!provider) return false;

  try {
    authStorage.remove(provider.id);
    const label =
      provider.authType === "oauth"
        ? `Logged out of ${provider.name}`
        : `Removed stored API key for ${provider.name}. Environment variables and models.json config are unchanged.`;
    ctx.msg("system", label);
  } catch (e: unknown) {
    ctx.msg("system", `Logout failed: ${e instanceof Error ? e.message : String(e)}`);
  }
  ctx.render();
  return true;
}

// ============================================================================
// Main entry point
// ============================================================================

/**
 * Open the login flow — mirrors pi's `/login` command.
 */
export async function openLoginFlow(
  ctx: OverlayContext,
  authStorage: AuthStorage = AuthStorage.create(),
): Promise<boolean> {
  const t = getTheme();

  // ---- Step 1: Auth type selector ----
  const authType = await showSelectList(ctx, "Select authentication method:", [
    {
      value: "oauth",
      label: "Use a subscription",
      description: t.fg("muted", "OAuth / device code flow"),
    },
    {
      value: "api_key",
      label: "Use an API key",
      description: t.fg("muted", "Enter your provider API key"),
    },
  ]);

  if (!authType) return false;

  // ---- Step 2: Provider selector ----
  const providerOptions = buildProviderOptions(authStorage, authType as AuthType);

  if (providerOptions.length === 0) {
    ctx.msg(
      "system",
      authType === "oauth"
        ? "No subscription providers available."
        : "No API key providers available.",
    );
    ctx.render();
    return false;
  }

  const providerItems: SelectItem[] = providerOptions.map((p) => ({
    value: p.id,
    label: p.name,
    description: p.configured ? t.fg("success", "✓ configured") : t.fg("dim", "not configured"),
  }));

  const providerId = await showSelectList(
    ctx,
    authType === "oauth" ? "Select subscription provider:" : "Select API key provider:",
    providerItems,
  );

  if (!providerId) {
    // User went back — restart from step 1
    return openLoginFlow(ctx, authStorage);
  }

  const provider = providerOptions.find((p) => p.id === providerId);
  if (!provider) return false;

  // ---- Step 3: Authenticate ----
  if (authType === "oauth") {
    return openOAuthInDialog(ctx, provider.id, provider.name, authStorage);
  }
  return openApiKeyInDialog(ctx, provider.id, provider.name, authStorage);
}
