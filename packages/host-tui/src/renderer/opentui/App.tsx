// ============================================================================
// OpenTUI App Shell — composition only: providers, layout, keyboard bridge.
// Render plan computed by surface subsystem; slot/surface rendering delegated.
// ============================================================================

import { useKeyboard, useTerminalDimensions } from "@opentui/solid";
import type { KeyEvent } from "@opentui/core";
import { createEffect, createMemo, untrack } from "solid-js";
import type { PikoHost } from "piko-host-runtime";
import type { RunTuiOptions } from "../../app/types.js";
import { getDefaultTheme } from "../../theme/resolve.js";
import { applyLayoutPolicies } from "../../layout/policies.js";
import { selectStatus } from "../../state/selectors.js";
import { computeRenderPlan } from "../../surfaces/render-plan.js";
import { TuiController } from "../../runtime/tui-controller.js";
import { normalizeKeyEvent } from "../../focus/key-normalize.js";
import { ActionService } from "./action-service.js";
import { ThemeProvider } from "./theme-context.js";

import { PanelRenderer } from "./panels/PanelRenderer.js";
import { renderSlot } from "./SlotRenderer.js";
import { traceRender } from "./instrumentation.js";
import type { TuiStore } from "./store.js";

// ============================================================================
// Props
// ============================================================================

export interface AppProps {
  store: TuiStore;
  host: PikoHost;
  options?: RunTuiOptions;
  shutdown: () => void;
}

// ============================================================================
// App component
// ============================================================================

export function App(props: AppProps) {
  const { store, host } = props;
  const dims = useTerminalDimensions();

  // Stable ActionService
  const svc = createMemo(
    () => {
      return new ActionService(
        host,
        store,
        props.options?.modelRegistry,
        props.options?.settingsManager,
        props.shutdown,
      );
    },
    { equals: false },
  );
  const actionSvc = () => svc();

  // Create TuiController once
  const controller = createMemo(() => {
    return untrack(() => {
      const ctrl = new TuiController(host, store, props.shutdown);
      ctrl.setActionService(actionSvc());
      return ctrl;
    });
  }, { equals: false });
  const ctrl = () => controller();

  // Sync terminal dimensions (only dispatch on actual change)
  createEffect(() => {
    const d = dims();
    const current = store.state().layout.viewport;
    if (d.width && d.height && (d.width !== current.width || d.height !== current.height)) {
      store.dispatch({ type: "layout_resized", width: d.width, height: d.height });
    }
  });

  // Keyboard → TuiController
  useKeyboard((key: KeyEvent) => {
    const normalized = normalizeKeyEvent(key);
    if (!normalized) return;
    const handled = ctrl().handleKey(normalized);
    if (handled) {
      key.preventDefault();
      key.stopPropagation();
    }
  }, {});

  // Layout policies
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

  // === Render plan ===
  const state = store.state;
  const layout = () => state().layout;
  const statusContract = () => selectStatus(state());
  const isRunning = () => state().stream.status === "running";
  const blocking = () => state().surfaces.some((s) => "blocking" in s ? s.blocking : s.inputPolicy !== "passive");
  const timelineItems = () => state().timeline.items;
  const plan = () => computeRenderPlan(state());

  // Dev-only instrumentation: trace each render
  createEffect(() => {
    const s = state();
    traceRender({
      timelineItemCount: s.timeline.items.length,
      surfaceCount: s.surfaces.length,
      viewportWidth: s.layout.viewport.width,
      viewportHeight: s.layout.viewport.height,
    });
  });

  return (
    <ThemeProvider value={getDefaultTheme()}>
      <box flexDirection="column" width="100%" height="100%">
        {plan().inline.map((entry) => {
          if (entry.kind === "slot") {
            return renderSlot(entry.id, {
              timelineItems,
              layout,
              state,
              statusContract,
              isRunning,
              blocking,
              store,
              actionSvc: actionSvc(),
              ctrl: ctrl(),
            });
          }
          if (entry.kind === "surface") {
            return (
              <PanelRenderer
                surface={entry.surface! as any}
                store={store}
                controller={ctrl()}
                actionSvc={actionSvc()}
                host={host}
                settingsManager={props.options?.settingsManager}
              />
            );
          }
          return null;
        })}
      </box>
    </ThemeProvider>
  );
}
