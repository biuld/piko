/**
 * OAuth Login Dialog — device-code flow for provider authentication.
 *
 * Shows the verification URL and user code, polls for token completion,
 * and saves the OAuth credential to AuthStorage.
 */

import { Container, getKeybindings, Spacer, Text } from "@earendil-works/pi-tui";
import { AuthStorage, runDeviceCodeFlow } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

export function openOAuthDialog(ctx: OverlayContext, provider: string): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    const t = getTheme();
    const borderColor = (s: string) => t.fg("border", s);
    const authStorage = AuthStorage.create();

    let statusMessage = "";
    let userCode = "";
    let verificationUri = "";

    const overlayComp = new Container();

    function rebuild(): void {
      overlayComp.clear();
      overlayComp.addChild(new DynamicBorder(borderColor));
      overlayComp.addChild(new Text(t.fg("accent", t.bold(` OAuth Login — ${provider}`)), 1, 0));
      overlayComp.addChild(new Spacer(1));

      if (!userCode) {
        overlayComp.addChild(new Text(t.fg("muted", "Requesting device code..."), 1, 0));
      } else {
        overlayComp.addChild(new Text(t.fg("dim", "1. Open this URL in your browser:"), 1, 0));
        overlayComp.addChild(new Text(t.fg("accent", `   ${verificationUri}`), 1, 0));
        overlayComp.addChild(new Spacer(1));
        overlayComp.addChild(new Text(t.fg("dim", "2. Enter this code:"), 1, 0));
        overlayComp.addChild(new Text(t.fg("accent", t.bold(`   ${userCode}`)), 1, 0));
        overlayComp.addChild(new Spacer(1));
        if (statusMessage) {
          overlayComp.addChild(new Text(statusMessage, 1, 0));
        } else {
          overlayComp.addChild(new Text(t.fg("muted", "Waiting for authorization..."), 1, 0));
        }
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
          ctx.getActiveOverlay()?.hide();
          ctx.setActiveOverlay(null);
          resolve(false);
          return;
        }
      },
    });

    ctx.setActiveOverlay(
      ctx.tui.showOverlay(component, { anchor: "center", width: "60%", maxHeight: "50%" }),
    );

    // Start OAuth flow
    void runDeviceCodeFlow(provider, (uri: string, code: string) => {
      verificationUri = uri;
      userCode = code;
      statusMessage = "";
      rebuild();
      ctx.tui.requestRender();
    })
      .then((credential) => {
        authStorage.set(provider, credential);
        statusMessage = t.fg("success", "✓ OAuth authorized successfully");
        rebuild();
        ctx.tui.requestRender();
        // Close after a brief delay so the user sees success
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
        statusMessage = t.fg(
          "error",
          `OAuth failed: ${err instanceof Error ? err.message : String(err)}`,
        );
        rebuild();
        ctx.tui.requestRender();
        // Keep overlay open on error so user can read the message and cancel
        resolve(false);
      });
  });
}
