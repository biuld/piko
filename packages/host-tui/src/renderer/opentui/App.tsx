// ============================================================================
// OpenTUI App Shell — composition only: providers, layout, keyboard bridge.
// Render plan computed by surface subsystem; slot/surface rendering delegated.
// ============================================================================

import { useKeyboard, useTerminalDimensions } from "@opentui/solid";
import type { KeyEvent } from "@opentui/core";
import { createEffect, createMemo, createSignal, onCleanup, onMount, untrack, For } from "solid-js";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import type { PikoHost } from "piko-host-runtime";
import type { RunTuiOptions } from "../../app/types.js";
import type { ResolvedTuiTheme } from "../../theme/schema.js";
import { getDefaultTheme, setDefaultTheme } from "../../theme/resolve.js";
import { findPiThemes, loadPiThemeFile } from "../../theme/pi-theme-loader.js";
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
  const [statusClock, setStatusClock] = createSignal(Date.now());
  const statusContract = () => selectStatus(state(), statusClock());
  const isRunning = () => state().stream.status === "running";
  const blocking = () => state().surfaces.some((s) => "blocking" in s ? s.blocking : s.inputPolicy !== "passive");
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
      notificationExpiryTimer = setTimeout(() => {
        setStatusClock(Date.now());
      }, Math.max(0, nextExpiry - now));
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

  onMount(() => {
    // Discover and load external themes from .piko/themes/
    const dirs: string[] = [];
    const projectDir = path.join(process.cwd(), ".piko", "themes");
    const globalDir = path.join(os.homedir(), ".piko", "themes");
    if (fs.existsSync(projectDir)) dirs.push(projectDir);
    if (fs.existsSync(globalDir)) dirs.push(globalDir);

    if (dirs.length > 0) {
      const themes = findPiThemes(dirs);
      // Prefer "dark" theme; if not found, use first available
      const themeName = "dark";
      const filePath = themes.get(themeName) ?? themes.values().next().value;
      if (filePath) {
        try {
          const theme = loadPiThemeFile(filePath);
          setDefaultTheme(theme);
          setCurrentTheme(theme);
        } catch {
          // Keep default theme
        }
      }
    }
  });

  return (
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
          }}
        </For>
      </box>
    </ThemeProvider>
  );
}
