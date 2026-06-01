// ============================================================================
// OpenTUI App Shell
// Layout: chat (scrollbox), status line, editor (textarea), bottom bar, overlays
// ============================================================================

import { Portal, render, useKeyboard, useTerminalDimensions } from "@opentui/solid";
import type { KeyEvent } from "@opentui/core";
import { createEffect, createMemo } from "solid-js";
import type { PikoHost } from "piko-host-runtime";
import type { RunTuiOptions } from "../../app/types.js";
import { getDefaultTheme } from "../../theme/resolve.js";
import { applyLayoutPolicies } from "../../layout/policies.js";
import { selectStatusEntries } from "../../state/selectors.js";
import { ActionService } from "./action-service.js";
import { BottomBar } from "./BottomBar.js";
import { ChatView } from "./ChatView.js";
import { Editor } from "./Editor.js";
import { StatusLine } from "./StatusLine.js";
import { ThemeProvider } from "./theme-context.js";
import { LoginDialog } from "./overlays/LoginDialog.js";
import { ModelSelector } from "./overlays/ModelSelector.js";
import { ResumeSelector } from "./overlays/ResumeSelector.js";
import { SettingsSelector } from "./overlays/SettingsSelector.js";
import { ThinkingSelector } from "./overlays/ThinkingSelector.js";
import type { TuiStore } from "./store.js";

// ============================================================================
// Props
// ============================================================================

export interface AppProps {
  store: TuiStore;
  host: PikoHost;
  options?: RunTuiOptions;
}

// ============================================================================
// App component
// ============================================================================

export function App(props: AppProps) {
  const { store, host } = props;
  const dims = useTerminalDimensions();

  // Stable ActionService — survives Solid re-renders
  const svc = createMemo(
    () =>
      new ActionService(
        host,
        store,
        props.options?.modelRegistry,
        props.options?.settingsManager,
      ),
    { equals: false },
  );
  const actionSvc = () => svc();

  // Sync terminal dimensions to state
  createEffect(() => {
    const d = dims();
    if (d.width && d.height) {
      store.dispatch({
        type: "layout_resized",
        width: d.width,
        height: d.height,
      });
    }
  });

  // Keyboard handling — global shortcuts
  useKeyboard((key: KeyEvent) => {
    const current = store.state();

    // When an overlay is open, only handle Esc
    if (current.overlay) {
      if (key.name === "escape") {
        store.dispatch({ type: "overlay_closed" });
      }
      return;
    }

    // Ctrl+C — abort if running, otherwise exit
    if (key.name === "c" && key.ctrl) {
      if (current.stream.status === "running") {
        actionSvc().abortRun();
      }
      return;
    }

    // Ctrl+D — exit when idle and editor empty
    if (key.name === "d" && key.ctrl) {
      if (current.stream.status !== "running") {
        actionSvc().shutdown();
      }
      return;
    }

    // Ctrl+P — open model selector
    if (key.name === "p" && key.ctrl) {
      store.dispatch({
        type: "overlay_opened",
        overlay: { kind: "model", isOpen: true, placement: "modal" },
      });
      return;
    }

    // Ctrl+N — open model selector
    if (key.name === "n" && key.ctrl) {
      store.dispatch({
        type: "overlay_opened",
        overlay: { kind: "model", isOpen: true, placement: "modal" },
      });
      return;
    }

    // Ctrl+T — open thinking selector
    if (key.name === "t" && key.ctrl) {
      store.dispatch({
        type: "overlay_opened",
        overlay: { kind: "thinking", isOpen: true, placement: "modal" },
      });
      return;
    }

    // Ctrl+R — open resume selector
    if (key.name === "r" && key.ctrl) {
      if (current.stream.status !== "running") {
        store.dispatch({
          type: "overlay_opened",
          overlay: { kind: "resume", isOpen: true, placement: "modal" },
        });
      }
      return;
    }
  }, {});

  // Apply layout policies when state changes
  createEffect(() => {
    const current = store.state();
    const updated = applyLayoutPolicies(current);
    if (updated !== current) {
      if (
        updated.layout.mode !== current.layout.mode ||
        updated.layout.activeRegion !== current.layout.activeRegion ||
        updated.layout.bottomBar.density !== current.layout.bottomBar.density
      ) {
        store.setState(updated);
      }
    }
  });

  // Derive view models from state
  const state = store.state;
  const layout = () => state().layout;
  const mode = () => layout().mode;
  const statusEntries = () => selectStatusEntries(state());
  const overlay = () => state().overlay;
  const isRunning = () => state().stream.status === "running";
  const isOverlay = () => overlay() !== null;
  const overlayPlacement = () => overlay()?.placement ?? "modal";

  // Editor visibility: hide for modal overlays, show for drawer
  const showEditor = () => !isOverlay() || overlayPlacement() === "drawer";

  // Status line height: always reserve space to avoid layout jumps
  const statusHeight = () => {
    const entries = statusEntries();
    return entries.length > 0 ? 1 : 0;
  };

  return (
    <ThemeProvider value={getDefaultTheme()}>
    <box flexDirection="column" width="100%" height="100%">
      {/* Chat area — scrollbox for message list */}
      <box flexGrow={1} flexShrink={1} overflow="hidden">
        <ChatView
          transcript={state().transcript}
          mode={mode()}
          isStreaming={isRunning()}
        />
      </box>

      {/* Status line — stable height region */}
      <box flexShrink={0} height={statusHeight()}>
        <StatusLine entries={statusEntries()} visible={statusEntries().length > 0} />
      </box>

      {/* Editor input */}
      {showEditor() && (
        <box flexShrink={0} height={mode() === "minimal" ? 3 : mode() === "compact" ? 5 : 10}>
          <Editor actionSvc={actionSvc()} disabled={isRunning()} />
        </box>
      )}

      {/* Bottom bar */}
      <box flexShrink={0} height={mode() === "minimal" ? 1 : mode() === "compact" ? 2 : 4}>
        <BottomBar store={store} />
      </box>

      {/* Overlays — rendered via Portal */}
      {isOverlay() && (
        <Portal>
          {overlay()!.kind === "model" && (
            <ModelSelector
              actionSvc={actionSvc()}
              onClose={() => store.dispatch({ type: "overlay_closed" })}
            />
          )}
          {overlay()!.kind === "thinking" && (
            <ThinkingSelector
              actionSvc={actionSvc()}
              onClose={() => store.dispatch({ type: "overlay_closed" })}
            />
          )}
          {overlay()!.kind === "resume" && (
            <ResumeSelector
              actionSvc={actionSvc()}
              onClose={() => store.dispatch({ type: "overlay_closed" })}
            />
          )}
          {overlay()!.kind === "settings" && (
            <SettingsSelector
              store={store}
              settingsManager={props.options?.settingsManager}
              onClose={() => store.dispatch({ type: "overlay_closed" })}
            />
          )}
          {overlay()!.kind === "login" && (
            <LoginDialog
              store={store}
              provider={state().model.current.provider}
              onClose={() => store.dispatch({ type: "overlay_closed" })}
            />
          )}
        </Portal>
      )}
    </box>
    </ThemeProvider>
  );
}

// ============================================================================
// Entry point
// ============================================================================

export async function runOpenTui(
  store: TuiStore,
  host: PikoHost,
  options?: RunTuiOptions,
): Promise<void> {
  await render(() => <App store={store} host={host} options={options} />);
}
