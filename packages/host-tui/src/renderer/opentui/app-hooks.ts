import type { KeyEvent } from "@opentui/core";
import { type Accessor, createEffect, createSignal, onCleanup } from "solid-js";
import type { TuiHostFacade, TuiOrchState } from "../../app/tui-host.js";
import { normalizeKeyEvent } from "../../focus/key-normalize.js";
import { applyLayoutPolicies } from "../../layout/policies.js";
import type { TuiController } from "../../runtime/tui-controller.js";
import { joinPath } from "../../shared/index.js";
import type { TuiState } from "../../state/state.js";
import { findPiThemes, loadPiThemeFile } from "../../theme/pi-theme-loader.js";
import { getDefaultTheme, setDefaultTheme } from "../../theme/resolve.js";
import type { ResolvedTuiTheme } from "../../theme/schema.js";
import { traceRender } from "./instrumentation.js";
import type { TuiStore } from "./store.js";

function homeDir(): string {
  return process.env.HOME ?? process.env.USERPROFILE ?? ".";
}

export function useViewportSync(
  store: TuiStore,
  dimensions: Accessor<{ width: number; height: number }>,
): void {
  createEffect(() => {
    const next = dimensions();
    const current = store.state().layout.viewport;
    if (
      next.width &&
      next.height &&
      (next.width !== current.width || next.height !== current.height)
    ) {
      store.dispatch({ type: "layout_resized", width: next.width, height: next.height });
    }
  });
}

export function useKeyboardBridge(
  useKeyboard: (handler: (key: KeyEvent) => void, options: Record<string, never>) => void,
  controller: Accessor<TuiController>,
): void {
  useKeyboard((key: KeyEvent) => {
    const normalized = normalizeKeyEvent(key);
    if (!normalized) return;
    const handled = controller().handleKey(normalized);
    if (handled) {
      key.preventDefault();
      key.stopPropagation();
    }
  }, {});
}

export function useLayoutPolicies(store: TuiStore): void {
  createEffect(() => {
    const current = store.state();
    const updated = applyLayoutPolicies(current);
    if (updated === current) return;
    if (
      updated.layout.mode !== current.layout.mode ||
      updated.layout.activeRegion !== current.layout.activeRegion ||
      updated.layout.bottomBar.density !== current.layout.bottomBar.density
    ) {
      store.setState(updated);
    }
  });
}

export function useStatusClock(state: Accessor<TuiState>): Accessor<number> {
  const [statusClock, setStatusClock] = createSignal(Date.now());
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
        () => setStatusClock(Date.now()),
        Math.max(0, nextExpiry - now),
      );
    }
  });

  onCleanup(() => {
    if (notificationExpiryTimer) clearTimeout(notificationExpiryTimer);
  });

  return statusClock;
}

export function useSpinnerFrame(intervalMs = 80): Accessor<number> {
  const [spinnerFrame, setSpinnerFrame] = createSignal(0);
  const spinnerTimer = setInterval(() => setSpinnerFrame((frame) => frame + 1), intervalMs);
  onCleanup(() => clearInterval(spinnerTimer));
  return spinnerFrame;
}

export function useOrchestratorSnapshot(host: TuiHostFacade): Accessor<TuiOrchState | undefined> {
  const [orchestratorSnapshot, setOrchestratorSnapshot] = createSignal<TuiOrchState>();
  let snapshotTimer: ReturnType<typeof setInterval> | undefined;

  if (host.teamMode) {
    let snapshotSignature = "";
    const refreshSnapshot = () => {
      const snapshot = host.getOrchestratorSnapshot();
      const nextSignature = JSON.stringify(snapshot);
      if (nextSignature === snapshotSignature) return;
      snapshotSignature = nextSignature;
      setOrchestratorSnapshot(snapshot);
    };
    refreshSnapshot();
    snapshotTimer = setInterval(refreshSnapshot, 100);
  }

  onCleanup(() => {
    if (snapshotTimer) clearInterval(snapshotTimer);
  });

  return orchestratorSnapshot;
}

export function usePiTheme(themeName: Accessor<string>): Accessor<ResolvedTuiTheme> {
  const [currentTheme, setCurrentTheme] = createSignal<ResolvedTuiTheme>(getDefaultTheme());

  createEffect(() => {
    const name = themeName();
    const dirs = [
      joinPath(process.cwd(), ".piko", "themes"),
      joinPath(homeDir(), ".piko", "themes"),
    ];

    void (async () => {
      const themes = await findPiThemes(dirs);
      const filePath = themes.get(name) ?? themes.values().next().value;
      if (!filePath) return;
      try {
        const theme = await loadPiThemeFile(filePath);
        setDefaultTheme(theme);
        setCurrentTheme(theme);
      } catch {
        // Keep default theme.
      }
    })();
  });

  return currentTheme;
}

export function useRenderTrace(state: Accessor<TuiState>): void {
  createEffect(() => {
    const s = state();
    traceRender({
      timelineItemCount: s.timeline.items.length,
      surfaceCount: s.surfaces.length,
      viewportWidth: s.layout.viewport.width,
      viewportHeight: s.layout.viewport.height,
    });
  });
}
