/**
 * OAuth Login Dialog — uses authStorage.login() with the full OAuth provider flow.
 *
 * Supports: device-code, browser callback, manual code paste, provider-specific flows.
 * Cancellation via Escape (AbortSignal).
 */

import { Container, getKeybindings, Input, Spacer, Text } from "@earendil-works/pi-tui";
import { AuthStorage } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

export function openOAuthDialog(
  ctx: OverlayContext,
  provider: string,
  authStorage: AuthStorage = AuthStorage.create(),
): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    const t = getTheme();
    const borderColor = (s: string) => t.fg("border", s);
    const abortController = new AbortController();

    let statusMessage = "";
    let flowDone = false;
    let inputResolver: ((value: string) => void) | undefined;
    let inputRejecter: ((error: Error) => void) | undefined;

    const overlayComp = new Container();

    // Input for prompts (API key, manual code, etc.)
    const input = new Input();
    input.onSubmit = () => {
      if (inputResolver) {
        inputResolver(input.getValue());
        inputResolver = undefined;
        inputRejecter = undefined;
      }
    };
    input.onEscape = () => {
      if (inputRejecter) {
        inputRejecter(new Error("Login cancelled"));
        inputResolver = undefined;
        inputRejecter = undefined;
      }
    };

    function promptForInput(_message: string, _placeholder?: string): Promise<string> {
      return new Promise((resolvePrompt, rejectPrompt) => {
        inputResolver = resolvePrompt;
        inputRejecter = rejectPrompt;
        input.setValue("");
        rebuild();
        ctx.tui.requestRender();
      });
    }

    function rebuild(): void {
      overlayComp.clear();
      overlayComp.addChild(new DynamicBorder(borderColor));
      overlayComp.addChild(new Text(t.fg("accent", t.bold(` OAuth Login — ${provider}`)), 1, 0));
      overlayComp.addChild(new Spacer(1));

      overlayComp.addChild(
        new Text(statusMessage || t.fg("muted", "Starting authentication..."), 1, 0),
      );

      if (inputResolver) {
        overlayComp.addChild(new Spacer(1));
        overlayComp.addChild(input);
        overlayComp.addChild(
          new Text(
            `(${keyHint("tui.select.cancel", "cancel,")} ${keyHint("tui.select.confirm", "submit")})`,
            1,
            0,
          ),
        );
      }

      overlayComp.addChild(new Spacer(1));
      overlayComp.addChild(new Text(`${keyHint("tui.select.cancel", "cancel")}`, 1, 0));
      overlayComp.addChild(new DynamicBorder(borderColor));
    }

    rebuild();

    const component = makeFocusable(overlayComp);
    Object.assign(component, {
      handleInput(data: string) {
        const kb = getKeybindings();

        if (kb.matches(data, "tui.select.cancel")) {
          if (!flowDone) {
            abortController.abort();
          }
          ctx.getActiveOverlay()?.hide();
          ctx.setActiveOverlay(null);
          resolve(false);
          return;
        }

        // Pass to input if prompt is active
        if (inputResolver) {
          input.handleInput(data);
          ctx.tui.requestRender();
          return;
        }
      },
      get focused() {
        return true;
      },
      set focused(_v: boolean) {},
    });

    ctx.setActiveOverlay(ctx.showReplacement(component));

    // Use authStorage.login() for full OAuth provider flow
    void authStorage
      .login(provider, {
        onAuth: (info) => {
          statusMessage = t.fg("accent", `Open in browser:\n${info.url}`);
          if (info.instructions) {
            statusMessage += `\n${t.fg("warning", info.instructions)}`;
          }
          rebuild();
          ctx.tui.requestRender();
        },
        onDeviceCode: (info) => {
          statusMessage = [
            t.fg("dim", "1. Open this URL in your browser:"),
            t.fg("accent", `   ${info.verificationUri}`),
            "",
            t.fg("dim", "2. Enter this code:"),
            t.fg("accent", t.bold(`   ${info.userCode}`)),
            "",
            t.fg("muted", "Waiting for authorization..."),
          ].join("\n");
          rebuild();
          ctx.tui.requestRender();
        },
        onPrompt: async (prompt) => {
          statusMessage = prompt.message;
          return promptForInput(prompt.message, prompt.placeholder);
        },
        onProgress: (message) => {
          statusMessage = t.fg("dim", message);
          rebuild();
          ctx.tui.requestRender();
        },
        onManualCodeInput: async () => {
          statusMessage = "Paste redirect URL below, or complete login in browser:";
          return promptForInput("Paste redirect URL below, or complete login in browser:");
        },
        onSelect: async (prompt) => {
          statusMessage = `${prompt.message}\nType: ${prompt.options.map((o) => o.id).join(" or ")}`;
          const result = await promptForInput(prompt.message);
          const trimmed = result.trim().toLowerCase();
          if (prompt.options.some((o) => o.id === trimmed)) return trimmed;
          return prompt.options[0]?.id ?? "";
        },
        signal: abortController.signal,
      })
      .then(() => {
        flowDone = true;
        statusMessage = t.fg("success", "✓ OAuth authorized successfully");
        rebuild();
        ctx.tui.requestRender();
        setTimeout(() => {
          const active = ctx.getActiveOverlay();
          if (active) active.hide();
          ctx.setActiveOverlay(null);
          ctx.msg("system", `OAuth login successful for ${provider}.`);
          ctx.render();
          resolve(true);
        }, 1500);
      })
      .catch((err: unknown) => {
        flowDone = true;
        const msg = err instanceof Error ? err.message : String(err);
        statusMessage = t.fg(msg.includes("cancelled") ? "muted" : "error", `OAuth failed: ${msg}`);
        rebuild();
        ctx.tui.requestRender();
        resolve(false);
      });
  });
}
