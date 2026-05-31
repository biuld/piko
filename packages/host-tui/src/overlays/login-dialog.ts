/**
 * Login Dialog — API key entry overlay for providers.
 *
 * Supports:
 * - Entering/saving API keys per provider
 * - Listing configured providers
 * - Removing stored keys
 *
 * Returns true if the user saved a key, false if cancelled.
 */

import { Container, getKeybindings, Input, Spacer, Text } from "@earendil-works/pi-tui";
import { AuthStorage, getOAuthConfig } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint, rawKeyHint } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";
import { openOAuthDialog } from "./oauth-dialog.js";

export function openLoginDialog(
  ctx: OverlayContext,
  provider: string,
  authStorage: AuthStorage = AuthStorage.create(),
): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    const t = getTheme();
    const borderColor = (s: string) => t.fg("border", s);

    const apiKeyInput = new Input();
    apiKeyInput.setValue("");
    let statusMessage = "";

    // Check existing auth
    const existingCred = authStorage.get(provider);
    if (existingCred?.type === "api_key") {
      statusMessage = t.fg("success", `✓ API key configured for ${provider}`);
      apiKeyInput.setValue(existingCred.key);
    } else {
      statusMessage = t.fg("muted", `No API key configured for ${provider}`);
    }

    const overlayComp = new Container();
    const oauthAvailable = !!getOAuthConfig(provider);

    function rebuild(): void {
      overlayComp.clear();
      overlayComp.addChild(new DynamicBorder(borderColor));
      overlayComp.addChild(new Text(t.fg("accent", t.bold(` Login — ${provider}`)), 1, 0));
      overlayComp.addChild(new Spacer(1));
      overlayComp.addChild(new Text(t.fg("dim", "API Key:"), 1, 0));
      overlayComp.addChild(apiKeyInput);
      overlayComp.addChild(new Spacer(1));
      if (statusMessage) {
        overlayComp.addChild(new Text(statusMessage, 1, 0));
        overlayComp.addChild(new Spacer(1));
      }
      overlayComp.addChild(
        new Text(
          `${keyHint("tui.input.submit", "save")}  ${keyHint("tui.select.cancel", "cancel")}  ${rawKeyHint("Ctrl+D", "remove")}${oauthAvailable ? `  ${rawKeyHint("Ctrl+O", "OAuth login")}` : ""}`,
          1,
          0,
        ),
      );
      overlayComp.addChild(new DynamicBorder(borderColor));
    }

    function _saveKey(): void {
      const key = apiKeyInput.getValue().trim();
      if (key) {
        authStorage.set(provider, { type: "api_key", key });
        statusMessage = t.fg("success", `✓ API key saved for ${provider}`);
      }
    }

    function _removeKey(): void {
      authStorage.remove(provider);
      apiKeyInput.setValue("");
      statusMessage = t.fg("muted", `API key removed for ${provider}`);
      rebuild();
      ctx.tui.requestRender();
    }

    rebuild();

    const component = makeFocusable(overlayComp, apiKeyInput);
    Object.assign(component, {
      handleInput(data: string) {
        const kb = getKeybindings();

        if (kb.matches(data, "tui.input.submit")) {
          const key = apiKeyInput.getValue().trim();
          if (key) {
            authStorage.set(provider, { type: "api_key", key });
            statusMessage = t.fg("success", `✓ API key saved for ${provider}`);
            ctx.getActiveOverlay()?.hide();
            ctx.setActiveOverlay(null);
            resolve(true);
          }
          return;
        }

        if (kb.matches(data, "tui.select.cancel")) {
          ctx.getActiveOverlay()?.hide();
          ctx.setActiveOverlay(null);
          resolve(false);
          return;
        }

        // Ctrl+O to switch to OAuth flow
        if (data === "\u000f" && oauthAvailable) {
          ctx.getActiveOverlay()?.hide();
          ctx.setActiveOverlay(null);
          void openOAuthDialog(ctx, provider, authStorage).then(resolve);
          return;
        }

        // Ctrl+D to remove key
        if (data === "\u0004") {
          authStorage.remove(provider);
          statusMessage = t.fg("muted", `API key removed for ${provider}`);
          apiKeyInput.setValue("");
          rebuild();
          ctx.tui.requestRender();
          return;
        }

        apiKeyInput.handleInput(data);
        rebuild();
        ctx.tui.requestRender();
      },
    });

    ctx.setActiveOverlay(
      ctx.tui.showOverlay(component, { anchor: "center", width: "60%", maxHeight: "40%" }),
    );
  });
}
