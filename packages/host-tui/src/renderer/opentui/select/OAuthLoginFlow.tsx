// ============================================================================
// OAuthLoginFlow — drives the OAuth login interaction.
//
// On mount, calls AuthStorage.login() with callbacks that update SolidJS
// signals. Renders status messages, URL, and instructions during the flow.
// Supports interactive phases (select, prompt) for providers that need them.
// Supports Esc to cancel. Auto-closes on success/error via onComplete.
// ============================================================================

import type { TextareaRenderable } from "@opentui/core";
import { createSignal, onCleanup, onMount } from "solid-js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { AuthStorage } from "../../../shared/index.js";
import { openBrowser } from "../../../utils/open-browser.js";
import { useTheme } from "../theme-context.js";

export interface OAuthLoginFlowProps {
  provider: string;
  providerName: string;
  authStorage: AuthStorage;
  controller: TuiController;
  surfaceId: string;
  onComplete: (success: boolean, message?: string) => void;
}

// ============================================================================
// Flow state machine
// ============================================================================

type FlowStatus =
  | { phase: "connecting" }
  | { phase: "browser"; url: string; instructions?: string }
  | { phase: "device_code"; userCode: string; verificationUri: string; instructions?: string }
  | { phase: "progress"; message: string }
  | { phase: "select"; message: string; options: { id: string; label: string }[] }
  | { phase: "prompt"; message: string; placeholder?: string }
  | { phase: "error"; message: string };

// ============================================================================
// Component
// ============================================================================

export function OAuthLoginFlow(props: OAuthLoginFlowProps) {
  const { provider, providerName, authStorage, controller, surfaceId, onComplete } = props;

  const [status, setStatus] = createSignal<FlowStatus>({ phase: "connecting" });
  const [selectIndex, setSelectIndex] = createSignal(0);
  const [promptText, setPromptText] = createSignal("");

  // Mutable refs for async Promise resolution
  const pending = {
    selectResolve: undefined as ((id: string | undefined) => void) | undefined,
    promptResolve: undefined as ((value: string) => void) | undefined,
    promptReject: undefined as ((err: Error) => void) | undefined,
  };

  let ac: AbortController;
  let textareaRef: TextareaRenderable | undefined;

  // ==========================================================================
  // OAuth callback implementations
  // ==========================================================================

  const callbacks = {
    onAuth(info: { url: string; instructions?: string }) {
      setStatus({ phase: "browser", url: info.url, instructions: info.instructions });
      openBrowser(info.url);
    },

    onDeviceCode(info: {
      userCode: string;
      verificationUri: string;
      intervalSeconds?: number;
      expiresInSeconds?: number;
    }) {
      setStatus({
        phase: "device_code",
        userCode: info.userCode,
        verificationUri: info.verificationUri,
        instructions: "Waiting for device authorization...",
      });
      openBrowser(info.verificationUri);
    },

    onProgress(message: string) {
      setStatus({ phase: "progress", message });
    },

    onPrompt(prompt: { message: string; placeholder?: string }): Promise<string> {
      return new Promise<string>((resolve, reject) => {
        pending.promptResolve = resolve;
        pending.promptReject = reject;
        setPromptText("");
        setStatus({ phase: "prompt", message: prompt.message, placeholder: prompt.placeholder });
      });
    },

    onSelect(prompt: {
      message: string;
      options: { id: string; label: string }[];
    }): Promise<string | undefined> {
      return new Promise<string | undefined>((resolve) => {
        pending.selectResolve = resolve;
        setSelectIndex(0);
        setStatus({ phase: "select", message: prompt.message, options: prompt.options });
      });
    },

    onManualCodeInput(): Promise<string> {
      return callbacks.onPrompt({
        message: "Paste the redirect URL or authorization code:",
        placeholder: "http://localhost:51121/oauth-callback?...",
      });
    },
  };

  // ==========================================================================
  // Keyboard handling
  // ==========================================================================

  function handleSelectKey(event: { name: string }): boolean {
    const s = status();
    if (s.phase !== "select") return false;

    const opts = s.options;
    if (event.name === "up") {
      setSelectIndex(Math.max(0, selectIndex() - 1));
      return true;
    }
    if (event.name === "down") {
      setSelectIndex(Math.min(opts.length - 1, selectIndex() + 1));
      return true;
    }
    if (event.name === "enter" || event.name === "return") {
      const idx = Math.min(selectIndex(), opts.length - 1);
      const resolve = pending.selectResolve;
      pending.selectResolve = undefined;
      resolve?.(opts[idx]?.id);
      return true;
    }
    return false;
  }

  function cancelInteractive(): boolean {
    const s = status();
    if (s.phase === "select") {
      const resolve = pending.selectResolve;
      pending.selectResolve = undefined;
      resolve?.(undefined);
      return true;
    }
    if (s.phase === "prompt") {
      const reject = pending.promptReject;
      pending.promptResolve = undefined;
      pending.promptReject = undefined;
      reject?.(new Error("Login cancelled"));
      return true;
    }
    return false;
  }

  // ==========================================================================
  // Mount
  // ==========================================================================

  onMount(() => {
    ac = new AbortController();

    controller.setSurfaceController(surfaceId, {
      handleKey(event) {
        // Select phase has its own navigation
        if (handleSelectKey(event)) {
          return { type: "handled" };
        }

        // Esc: cancel interactive or abort the whole flow
        if (event.name === "escape") {
          if (cancelInteractive()) {
            return { type: "handled" };
          }
          ac.abort();
          return { type: "handled" };
        }

        return { type: "unhandled" };
      },

      onConfirm(val?: unknown) {
        const s = status();

        // Prompt phase: submit textarea value
        if (s.phase === "prompt") {
          const value = typeof val === "string" ? val : (textareaRef?.plainText ?? promptText());
          const resolve = pending.promptResolve;
          pending.promptResolve = undefined;
          pending.promptReject = undefined;
          resolve?.(value || "");
          return;
        }

        // Select phase: confirm current selection
        if (s.phase === "select") {
          const opts = s.options;
          const idx = Math.min(selectIndex(), opts.length - 1);
          const resolve = pending.selectResolve;
          pending.selectResolve = undefined;
          resolve?.(opts[idx]?.id);
          return;
        }
      },
    });

    authStorage
      .login(provider, {
        onAuth: callbacks.onAuth,
        onDeviceCode: callbacks.onDeviceCode,
        onProgress: callbacks.onProgress,
        onPrompt: callbacks.onPrompt,
        onSelect: callbacks.onSelect,
        onManualCodeInput: callbacks.onManualCodeInput,
        signal: ac.signal,
      })
      .then(() => {
        onComplete(true, `Logged in to ${providerName}`);
      })
      .catch((err) => {
        if (ac.signal.aborted) {
          onComplete(false);
          return;
        }
        const msg = err instanceof Error ? err.message : String(err);
        setStatus({ phase: "error", message: msg });
        onComplete(false, msg);
      });
  });

  onCleanup(() => {
    controller.setSurfaceController(surfaceId, null);
  });

  // ==========================================================================
  // Render
  // ==========================================================================

  const theme = useTheme();

  return (
    <box flexDirection="column" padding={1} gap={1}>
      <StatusView
        status={status()}
        selectIndex={selectIndex()}
        promptText={promptText()}
        providerName={providerName}
        theme={theme}
        onPromptTextChange={(val: string) => setPromptText(val)}
        textareaRef={(ref: TextareaRenderable) => {
          textareaRef = ref;
        }}
      />
    </box>
  );
}

// ============================================================================
// StatusView — renders the appropriate UI for each flow phase
// ============================================================================

const themeColor = (theme: ReturnType<typeof useTheme>, name: string) => theme.color(name);

function StatusView(props: {
  status: FlowStatus;
  selectIndex: number;
  promptText: string;
  providerName: string;
  theme: ReturnType<typeof useTheme>;
  onPromptTextChange: (val: string) => void;
  textareaRef: (ref: TextareaRenderable) => void;
}) {
  const { status, selectIndex, promptText, providerName, theme, onPromptTextChange, textareaRef } =
    props;

  const dimText = (s: string) => <text fg={themeColor(theme, "text.dim")}>{s}</text>;
  const primaryText = (s: string) => <text fg={themeColor(theme, "text.primary")}>{s}</text>;
  const accentText = (s: string) => <text fg={themeColor(theme, "text.accent")}>{s}</text>;
  const errorText = (s: string) => <text fg={themeColor(theme, "text.error")}>{s}</text>;

  switch (status.phase) {
    case "connecting":
      return (
        <box flexDirection="column" gap={1}>
          {primaryText(`Connecting to ${providerName}...`)}
          {dimText("Press Esc to cancel")}
        </box>
      );

    case "browser":
      return (
        <box flexDirection="column" gap={1}>
          {accentText("Complete login in your browser:")}
          {dimText(status.url)}
          {status.instructions ? dimText(status.instructions) : null}
          {dimText("The browser should open automatically.")}
          {dimText("Press Esc to cancel")}
        </box>
      );

    case "device_code":
      return (
        <box flexDirection="column" gap={1}>
          {primaryText("Device code:")}
          {accentText(status.userCode)}
          {dimText(`Open ${status.verificationUri} and enter the code above.`)}
          {dimText(status.instructions ?? "Waiting for authorization...")}
          {dimText("Press Esc to cancel")}
        </box>
      );

    case "progress":
      return (
        <box flexDirection="column" gap={1}>
          {primaryText(status.message)}
          {dimText("Press Esc to cancel")}
        </box>
      );

    case "select":
      return (
        <box flexDirection="column" gap={1}>
          {primaryText(status.message)}
          <box marginTop={1} flexDirection="column" gap={0}>
            {status.options.map((opt, i) => {
              const isSelected = i === selectIndex;
              return (
                <text fg={themeColor(theme, isSelected ? "text.accent" : "text.primary")}>
                  {isSelected ? "> " : "  "}
                  {opt.label}
                </text>
              );
            })}
          </box>
          {dimText("↑↓ move  Enter select  Esc back")}
        </box>
      );

    case "prompt":
      return (
        <box flexDirection="column" gap={1}>
          {primaryText(status.message)}
          <box marginTop={1}>
            <textarea
              ref={textareaRef}
              focused={true}
              placeholder={status.placeholder ?? "Enter value..."}
              onContentChange={
                ((val: any) => {
                  const textValue = typeof val === "string" ? val : promptText || "";
                  onPromptTextChange(textValue);
                }) as any
              }
              onSubmit={
                ((val: any) => {
                  const textValue = typeof val === "string" ? val : promptText || "";
                  onPromptTextChange(textValue);
                  // onConfirm in surface controller handles resolution
                }) as any
              }
              keyBindings={[
                { name: "return", action: "submit" },
                { name: "kpenter", action: "submit" },
              ]}
            />
          </box>
          {dimText("Enter submit  Esc cancel")}
        </box>
      );

    case "error":
      return (
        <box flexDirection="column" gap={1}>
          {errorText("Login failed")}
          {dimText(status.message)}
          {dimText("Press Esc to close")}
        </box>
      );
  }
}
