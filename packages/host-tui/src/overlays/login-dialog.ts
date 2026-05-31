/**
 * Login Dialog Component — reusable auth UI (API key input + OAuth display).
 *
 * Used by the login flow for both API key and subscription (OAuth) authentication.
 * Features clickable URLs (OSC 8), auto-open browser, cancel with AbortSignal,
 * and IME cursor support via Focusable.
 */

import { exec } from "node:child_process";
import {
  Container,
  type Focusable,
  getKeybindings,
  Input,
  matchesKey,
  Spacer,
  Text,
  type TUI,
} from "@earendil-works/pi-tui";
import { DynamicBorder } from "../components/dynamic-border.js";
import { keyHint } from "../components/key-hints.js";
import { getTheme } from "../theme.js";

export type AuthType = "oauth" | "api_key";

export class LoginDialogComponent extends Container implements Focusable {
  private contentContainer: Container;
  private input: Input;
  private tui: TUI;
  private abortController = new AbortController();
  private inputResolver?: (value: string) => void;
  private inputRejecter?: (error: Error) => void;
  private onComplete: (success: boolean, message?: string) => void;

  // Focusable
  private _focused = false;
  get focused(): boolean {
    return this._focused;
  }
  set focused(value: boolean) {
    this._focused = value;
    this.input.focused = value;
  }

  constructor(
    tui: TUI,
    _providerId: string,
    providerName: string,
    authType: AuthType,
    onComplete: (success: boolean, message?: string) => void,
  ) {
    super();
    this.tui = tui;
    this.onComplete = onComplete;

    const t = getTheme();

    // Top border
    this.addChild(new DynamicBorder());

    // Title
    const typeLabel = authType === "oauth" ? "Subscription" : "API Key";
    const title = `Login to ${providerName} (${typeLabel})`;
    this.addChild(new Text(t.fg("accent", t.bold(title)), 1, 0));

    // Dynamic content area
    this.contentContainer = new Container();
    this.addChild(this.contentContainer);

    // Input (always present, used when needed)
    this.input = new Input();
    this.input.onSubmit = () => {
      if (this.inputResolver) {
        this.inputResolver(this.input.getValue());
        this.inputResolver = undefined;
        this.inputRejecter = undefined;
      }
    };
    this.input.onEscape = () => {
      this.cancel();
    };

    // Bottom border
    this.addChild(new DynamicBorder());
  }

  get signal(): AbortSignal {
    return this.abortController.signal;
  }

  /** Cancel the current flow */
  cancel(): void {
    this.abortController.abort();
    if (this.inputRejecter) {
      this.inputRejecter(new Error("Login cancelled"));
      this.inputResolver = undefined;
      this.inputRejecter = undefined;
    }
    this.onComplete(false, "Login cancelled");
  }

  // ---- Display methods ----

  /** Show OAuth authorization URL */
  showAuth(url: string, instructions?: string): void {
    const t = getTheme();
    this.contentContainer.clear();
    this.contentContainer.addChild(new Spacer(1));

    const linkedUrl = `\x1b]8;;${url}\x07${url}\x1b]8;;\x07`;
    this.contentContainer.addChild(new Text(t.fg("accent", linkedUrl), 1, 0));

    const clickHint = process.platform === "darwin" ? "Cmd+click to open" : "Ctrl+click to open";
    const hyperlink = `\x1b]8;;${url}\x07${clickHint}\x1b]8;;\x07`;
    this.contentContainer.addChild(new Text(t.fg("dim", hyperlink), 1, 0));

    if (instructions) {
      this.contentContainer.addChild(new Spacer(1));
      this.contentContainer.addChild(new Text(t.fg("warning", instructions), 1, 0));
    }

    this.contentContainer.addChild(new Spacer(1));
    this.contentContainer.addChild(new Text(`(${keyHint("tui.select.cancel", "cancel")})`, 1, 0));

    this.openUrl(url);
    this.tui.requestRender();
  }

  /** Show OAuth device code */
  showDeviceCode(verificationUri: string, userCode: string): void {
    const t = getTheme();
    this.contentContainer.clear();
    this.contentContainer.addChild(new Spacer(1));
    this.contentContainer.addChild(
      new Text(t.fg("dim", "1. Open this URL in your browser:"), 1, 0),
    );

    const linkedUrl = `\x1b]8;;${verificationUri}\x07${verificationUri}\x1b]8;;\x07`;
    this.contentContainer.addChild(new Text(t.fg("accent", `   ${linkedUrl}`), 1, 0));

    const clickHint = process.platform === "darwin" ? "Cmd+click to open" : "Ctrl+click to open";
    const hyperlink = `\x1b]8;;${verificationUri}\x07${clickHint}\x1b]8;;\x07`;
    this.contentContainer.addChild(new Text(t.fg("dim", `   ${hyperlink}`), 1, 0));

    this.contentContainer.addChild(new Spacer(1));
    this.contentContainer.addChild(new Text(t.fg("dim", "2. Enter this code:"), 1, 0));
    this.contentContainer.addChild(new Text(t.fg("accent", t.bold(`   ${userCode}`)), 1, 0));
    this.contentContainer.addChild(new Spacer(1));
    this.contentContainer.addChild(new Text(t.fg("muted", "Waiting for authorization..."), 1, 0));
    this.contentContainer.addChild(new Spacer(1));
    this.contentContainer.addChild(new Text(`(${keyHint("tui.select.cancel", "cancel")})`, 1, 0));

    this.openUrl(verificationUri);
    this.tui.requestRender();
  }

  /** Show a prompt and wait for text input (API key, manual code, etc.) */
  showPrompt(message: string, placeholder?: string): Promise<string> {
    const t = getTheme();
    this.contentContainer.addChild(new Spacer(1));
    this.contentContainer.addChild(new Text(t.fg("text", message), 1, 0));
    if (placeholder) {
      this.contentContainer.addChild(new Text(t.fg("dim", `e.g., ${placeholder}`), 1, 0));
    }
    this.contentContainer.addChild(this.input);
    this.contentContainer.addChild(
      new Text(
        `(${keyHint("tui.select.cancel", "cancel,")} ${keyHint("tui.select.confirm", "submit")})`,
        1,
        0,
      ),
    );

    this.input.setValue("");
    this.tui.requestRender();

    return new Promise((resolve, reject) => {
      this.inputResolver = resolve;
      this.inputRejecter = reject;
    });
  }

  /** Show informational text */
  showInfo(lines: string[]): void {
    this.contentContainer.clear();
    this.contentContainer.addChild(new Spacer(1));
    for (const line of lines) {
      this.contentContainer.addChild(new Text(line, 1, 0));
    }
    this.contentContainer.addChild(new Spacer(1));
    this.contentContainer.addChild(new Text(`(${keyHint("tui.select.cancel", "close")})`, 1, 0));
    this.tui.requestRender();
  }

  /** Show waiting message (for polling flows) */
  showWaiting(message: string): void {
    const t = getTheme();
    this.contentContainer.addChild(new Spacer(1));
    this.contentContainer.addChild(new Text(t.fg("dim", message), 1, 0));
    this.contentContainer.addChild(new Text(`(${keyHint("tui.select.cancel", "cancel")})`, 1, 0));
    this.tui.requestRender();
  }

  /** Show progress message during OAuth polling */
  showProgress(message: string): void {
    const t = getTheme();
    this.contentContainer.addChild(new Text(t.fg("dim", message), 1, 0));
    this.tui.requestRender();
  }

  /** Auto-open URL in default browser */
  private openUrl(url: string): void {
    const openCmd =
      process.platform === "darwin" ? "open" : process.platform === "win32" ? "start" : "xdg-open";
    try {
      exec(`${openCmd} "${url}"`, () => {});
    } catch {
      // Ignore browser launch failures
    }
  }

  // ---- Input handling ----

  handleInput(data: string): void {
    const kb = getKeybindings();

    if (kb.matches(data, "tui.select.cancel")) {
      this.cancel();
      return;
    }

    if (matchesKey(data, "ctrl+d")) {
      this.cancel();
      return;
    }

    this.input.handleInput(data);
    this.tui.requestRender();
  }
}
