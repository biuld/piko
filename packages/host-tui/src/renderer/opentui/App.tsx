// ============================================================================
// OpenTUI App Shell
// Minimal layout: chat (scrollbox), status line, editor (textarea), bottom bar
// ============================================================================

import { render, useKeyboard, useTerminalDimensions } from "@opentui/solid";
import type { KeyEvent } from "@opentui/core";
import { createEffect } from "solid-js";
import type { PikoHost } from "piko-host-runtime";
import type { RunTuiOptions } from "../../app/types.js";
import { applyLayoutPolicies } from "../../layout/policies.js";
import { selectStatusEntries } from "../../state/selectors.js";
import { submitPrompt } from "../../state/actions.js";
import type { ActionContext } from "../../state/actions.js";
import type { TuiEvent } from "../../state/events.js";
import { BottomBar } from "./BottomBar.js";
import { ChatView } from "./ChatView.js";
import { Editor } from "./Editor.js";
import { StatusLine } from "./StatusLine.js";
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

  // Create action context
  const actionCtx: ActionContext = {
    host,
    dispatch: (event: TuiEvent) => store.dispatch(event),
    getState: () => store.state(),
  };

  // Keyboard handling
  useKeyboard((key: KeyEvent) => {
    // Ctrl+C to abort
    if (key.name === "c" && key.ctrl) {
      const current = store.state();
      if (current.stream.status === "running") {
        actionCtx.abortController?.abort();
        store.dispatch({ type: "aborted" });
      }
    }
  }, {});

  // Apply layout policies when state changes
  createEffect(() => {
    const current = store.state();
    const updated = applyLayoutPolicies(current);
    if (updated !== current) {
      // Only update if layout actually changed (avoid infinite loop)
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

  return (
    <box flexDirection="column" width="100%" height="100%">
      {/* Chat area — scrollbox for message list */}
      <box flexGrow={1} flexShrink={1} overflow="hidden">
        <ChatView
          transcript={state().transcript}
          mode={mode()}
          isStreaming={state().stream.status === "running"}
        />
      </box>

      {/* Status line — shows during streaming */}
      <StatusLine entries={statusEntries()} visible={statusEntries().length > 0} />

      {/* Editor input */}
      <box flexShrink={0} height={mode() === "minimal" ? 3 : mode() === "compact" ? 5 : 10}>
        <Editor
          store={store}
          actionCtx={actionCtx}
          disabled={state().stream.status === "running"}
        />
      </box>

      {/* Bottom bar */}
      <box flexShrink={0} height={mode() === "minimal" ? 1 : mode() === "compact" ? 2 : 4}>
        <BottomBar store={store} />
      </box>
    </box>
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
