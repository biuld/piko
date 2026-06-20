// ============================================================================
// OpenTUI App Shell — composition only: providers, layout, keyboard bridge.
// Render plan computed by surface subsystem; slot/surface rendering delegated.
// ============================================================================

import type { KeyEvent } from "@opentui/core";
import { useKeyboard, useTerminalDimensions } from "@opentui/solid";
import { joinPath, type PikoHost } from "piko-host-runtime";
import { createEffect, createMemo, createSignal, For, onCleanup, untrack } from "solid-js";
import type { RunTuiOptions } from "../../app/types.js";
import { normalizeKeyEvent } from "../../focus/key-normalize.js";
import { applyLayoutPolicies } from "../../layout/policies.js";
import { TuiController } from "../../runtime/tui-controller.js";
import { selectStatus } from "../../state/selectors.js";
import { computeRenderPlan } from "../../surfaces/render-plan.js";
import { findPiThemes, loadPiThemeFile } from "../../theme/pi-theme-loader.js";
import { getDefaultTheme, setDefaultTheme } from "../../theme/resolve.js";
import type { ResolvedTuiTheme } from "../../theme/schema.js";
import { ActionService } from "./action-service.js";
import { traceRender } from "./instrumentation.js";
import { LayoutProvider } from "./layout-context.js";
import { PanelRenderer } from "./panels/PanelRenderer.js";
import { renderSlot } from "./SlotRenderer.js";
import type { TuiStore } from "./store.js";
import { ThemeProvider } from "./theme-context.js";

function homeDir(): string {
  return process.env.HOME ?? process.env.USERPROFILE ?? ".";
}

// ============================================================================
// Props
// ============================================================================

export interface AppProps {
  store: TuiStore;
  host: PikoHost;
  options?: RunTuiOptions;
  shutdown: () => void;
  controller?: TuiController;
  actionSvc?: ActionService;
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
      if (props.actionSvc) return props.actionSvc;
      return new ActionService(
        host,
        store,
        props.options!.settingsManager,
        props.options?.modelRegistry,
        props.shutdown,
      );
    },
    { equals: false },
  );
  const actionSvc = () => svc();

  // Create TuiController once
  const controller = createMemo(
    () => {
      if (props.controller) return props.controller;
      return untrack(() => {
        const ctrl = new TuiController(host, store, props.shutdown);
        ctrl.setActionService(actionSvc());
        return ctrl;
      });
    },
    { equals: false },
  );
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
  const [statusClock, setStatusClock] = createSignal(Date.now());
  const statusContract = () => selectStatus(state(), statusClock());
  const isRunning = () => state().stream.status === "running";
  const timelineItems = () => state().timeline.items;
  const plan = () => computeRenderPlan(state());

  let notificationExpiryTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const notifications = state().notifications;
    statusClock();
    const now = Date.now();
    if (notificationExpiryTimer) {
      clearTimeout(notificationExpiryTimer);
      notificationExpiryTimer = undefined;
    }

    const nextExpiry = notifications.reduce<number | undefined>((next, notification) => {
      if (notification.readAt || !notification.ttlMs) return next;
      const expiresAt = notification.createdAt + notification.ttlMs;
      if (expiresAt <= now) return next;
      return next === undefined || expiresAt < next ? expiresAt : next;
    }, undefined);

    if (nextExpiry !== undefined) {
      notificationExpiryTimer = setTimeout(
        () => {
          setStatusClock(Date.now());
        },
        Math.max(0, nextExpiry - now),
      );
    }
  });
  onCleanup(() => {
    if (notificationExpiryTimer) clearTimeout(notificationExpiryTimer);
  });

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

  // ---- Theme loading - from pi-format JSON files ----
  const [currentTheme, setCurrentTheme] = createSignal<ResolvedTuiTheme>(getDefaultTheme());

  createEffect(() => {
    const themeName = state().layout.theme;
    const dirs: string[] = [];
    const projectDir = joinPath(process.cwd(), ".piko", "themes");
    const globalDir = joinPath(homeDir(), ".piko", "themes");
    dirs.push(projectDir, globalDir);

    void (async () => {
      if (dirs.length > 0) {
        const themes = await findPiThemes(dirs);
        const filePath = themes.get(themeName) ?? themes.values().next().value;
        if (filePath) {
          try {
            const theme = await loadPiThemeFile(filePath);
            setDefaultTheme(theme);
            setCurrentTheme(theme);
          } catch {
            // Keep default theme
          }
        }
      }
    })();
  });

  return (
    <LayoutProvider value={state().layout}>
      <ThemeProvider value={currentTheme()}>
        <box flexDirection="column" width="100%" height="100%">
          <For each={plan().inline}>
            {(entry) => {
              if (entry.kind === "slot") {
                return renderSlot(entry.id, {
                  timelineItems,
                  layout,
                  state,
                  statusContract,
                  isRunning,
                  store,
                  actionSvc: actionSvc(),
                  ctrl: ctrl(),
                  host: props.host,
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
            }}
          </For>
        </box>
      </ThemeProvider>
    </LayoutProvider>
  );
}
