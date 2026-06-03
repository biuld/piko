// ============================================================================
// OpenTUI App Shell — composition only: providers, layout, keyboard bridge
// Delegates all behavior to TuiController runtime subsystems.
// ============================================================================

import { Portal, useKeyboard, useTerminalDimensions } from "@opentui/solid";
import type { KeyEvent } from "@opentui/core";
import { createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { PikoHost } from "piko-host-runtime";
import type { RunTuiOptions } from "../../app/types.js";
import type { TuiState } from "../../state/state.js";
import type { KeyEvent as FocusKeyEvent } from "../../focus/types.js";
import type { SelectItem } from "./select/selector-controller.js";
import { getDefaultTheme } from "../../theme/resolve.js";
import { applyLayoutPolicies } from "../../layout/policies.js";
import { selectStatusEntries } from "../../state/selectors.js";
import { buildTimelineItems } from "../../timeline/timeline-builder.js";
import { TuiController } from "../../runtime/tui-controller.js";
import { ActionService } from "./action-service.js";
import { BottomBar } from "./BottomBar.js";
import { StatusLine } from "./StatusLine.js";
import { Editor } from "./Editor.js";
import { ThemeProvider } from "./theme-context.js";
import { LoginDialog } from "./LoginDialog.js";
import { ModelSelector } from "./select/ModelSelector.js";
import { ResumeSelector } from "./select/ResumeSelector.js";
import { SettingsSelector } from "./select/SettingsSelector.js";
import { ThinkingSelector } from "./select/ThinkingSelector.js";
import { SelectorShell } from "./select/SelectorShell.js";
import { SelectListView } from "./select/SelectListView.js";
import { TimelineView } from "./timeline/TimelineView.js";
import { SurfaceHost } from "./surfaces/SurfaceHost.js";
import type { TuiStore } from "./store.js";
import {
  createSelectableListState,
  getSelectedItem,
  handleSelectableListKey,
  type SelectableListState,
} from "../../surfaces/interactions/selectable-list.js";

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
    () =>
      new ActionService(
        host,
        store,
        props.options?.modelRegistry,
        props.options?.settingsManager,
        props.shutdown,
      ),
    { equals: false },
  );
  const actionSvc = () => svc();

  // Create TuiController once
  const controller = createMemo(() => {
    const ctrl = new TuiController(host, store, props.shutdown);
    ctrl.setActionService(actionSvc());
    return ctrl;
  }, { equals: false });
  const ctrl = () => controller();

  // Sync terminal dimensions
  createEffect(() => {
    const d = dims();
    if (d.width && d.height) {
      store.dispatch({ type: "layout_resized", width: d.width, height: d.height });
    }
  });

  // Keyboard handling routes through TuiController (which routes through focus + interceptors)
  useKeyboard((key: KeyEvent) => {
    const char =
      !key.ctrl &&
      !key.meta &&
      !(key as any).super &&
      !(key as any).hyper &&
      key.sequence &&
      key.sequence.length === 1 &&
      key.sequence >= " "
        ? key.sequence
        : undefined;
    const handled = ctrl().handleKey({
      name: key.name,
      ctrl: key.ctrl,
      shift: key.shift,
      alt: (key as any).option ?? false,
      meta: (key as any).meta ?? false,
      char,
    });
    if (handled) {
      key.preventDefault();
      key.stopPropagation();
    }
  }, {});

  // Apply layout policies
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

  // Derive view models
  const state = store.state;
  const layout = () => state().layout;
  const mode = () => layout().mode;
  const statusEntries = () => selectStatusEntries(state());
  const isRunning = () => state().stream.status === "running";
  const surfaces = () => state().surfaces;
  const timelineItems = () => buildTimelineItems(state().transcript);

  // Compute fully covered slots from all active surfaces
  const fullyCoveredSlots = () => {
    const slots = new Set<string>();
    for (const s of surfaces()) {
      for (const slot of s.occlusion.fullyCovers) {
        slots.add(slot);
      }
    }
    return slots;
  };

  const showTimeline = () => !fullyCoveredSlots().has("timeline") && !fullyCoveredSlots().has("app");
  const showEditor = () => !fullyCoveredSlots().has("editor") && !fullyCoveredSlots().has("app");
  const showStatus = () => !fullyCoveredSlots().has("status") && !fullyCoveredSlots().has("app");
  const showBottomBar = () => !fullyCoveredSlots().has("bottom-bar") && !fullyCoveredSlots().has("app");

  // Status line height
  const statusHeight = () => {
    if (!showStatus()) return 0;
    const entries = statusEntries();
    return entries.length > 0 ? 1 : 0;
  };

  return (
    <ThemeProvider value={getDefaultTheme()}>
    <box flexDirection="column" width="100%" height="100%">
      {/* Timeline / Chat area */}
      {showTimeline() && (
        <box flexGrow={1} flexShrink={1} overflow="hidden">
          <TimelineView
            items={timelineItems()}
            layout={{
              width: layout().viewport.width,
              height: layout().viewport.height,
              mode: mode(),
            }}
            pendingNewItems={state().timeline.pendingNewItems}
            stickyBottom={state().timeline.anchor === "bottom"}
            scrollCommand={state().scrollCommand ?? null}
            onScrollStateChange={(atBottom) => {
              store.dispatch({
                type: "chat_scrolled",
                anchor: atBottom ? "bottom" : "manual",
              });
            }}
            onScrollCommandDone={() => {
              store.setState((s) => ({ ...s, scrollCommand: null }));
            }}
            expandedItemIds={state().timeline.expandedItemIds}
            collapsedToolCallIds={state().timeline.collapsedToolCallIds}
          />
        </box>
      )}

      {/* insert-between surfaces after timeline */}
      {surfaces()
        .filter((s) => s.mount === "insert-between" && s.insertAfterSlot === "timeline")
        .map((s) => (
          <SurfaceHost surface={s}>
            {renderSurfaceContent(s, store, ctrl(), actionSvc(), props)}
          </SurfaceHost>
        ))}

      {/* Status line */}
      {showStatus() && (
        <box flexShrink={0} height={statusHeight()}>
          <StatusLine entries={statusEntries()} visible={statusEntries().length > 0} />
        </box>
      )}

      {/* insert-between surfaces after status */}
      {surfaces()
        .filter((s) => s.mount === "insert-between" && s.insertAfterSlot === "status")
        .map((s) => (
          <SurfaceHost surface={s}>
            {renderSurfaceContent(s, store, ctrl(), actionSvc(), props)}
          </SurfaceHost>
        ))}

      {/* Anchored autocomplete surfaces */}
      {surfaces()
        .filter((s) => s.mount === "anchored")
        .map((s) => (
          <SurfaceHost surface={s}>
            {renderSurfaceContent(s, store, ctrl(), actionSvc(), props)}
          </SurfaceHost>
        ))}

      {/* Editor */}
      {showEditor() && (
        <box flexShrink={0}>
          <Editor
            actionSvc={actionSvc()}
            controller={ctrl()}
            store={store}
            disabled={isRunning()}
            unfocused={surfaces().some((s) => s.blocking)}
          />
        </box>
      )}

      {/* insert-between surfaces after editor */}
      {surfaces()
        .filter((s) => s.mount === "insert-between" && s.insertAfterSlot === "editor")
        .map((s) => (
          <SurfaceHost surface={s}>
            {renderSurfaceContent(s, store, ctrl(), actionSvc(), props)}
          </SurfaceHost>
        ))}

      {/* Replace-slot surfaces: render in place of slots */}
      {surfaces()
        .filter((s) => s.mount === "replace-slot")
        .map((s) => (
          <SurfaceHost surface={s}>
            {renderSurfaceContent(s, store, ctrl(), actionSvc(), props)}
          </SurfaceHost>
        ))}

      {/* Bottom bar */}
      {showBottomBar() && (
        <box flexShrink={0} height={mode() === "minimal" ? 1 : 2}>
          <BottomBar store={store} />
        </box>
      )}

      {/* Side-drawer surfaces */}
      {surfaces()
        .filter((s) => s.mount === "side-drawer")
        .map((s) => (
          <Portal>
            <SurfaceHost surface={s}>
              {renderSurfaceContent(s, store, ctrl(), actionSvc(), props)}
            </SurfaceHost>
          </Portal>
        ))}
    </box>
    </ThemeProvider>
  );
}

// ============================================================================
// Surface content renderer
// ============================================================================

function renderSurfaceContent(
  surface: TuiState["surfaces"][0],
  store: TuiStore,
  ctrl: TuiController,
  actionSvc: ActionService,
  props: AppProps,
) {
  const data = surface.data as Record<string, unknown> | undefined;
  const surfaceType = data?.type as string | undefined;
  const surfaceId = surface.id;

  switch (surfaceType) {
    case "model":
      return (
        <ModelSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          onClose={() => ctrl.closeSurface(surface.id)}
        />
      );

    case "thinking":
      return (
        <ThinkingSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          onClose={() => ctrl.closeSurface(surface.id)}
        />
      );

    case "resume":
      return (
        <ResumeSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          onClose={() => ctrl.closeSurface(surface.id)}
        />
      );

    case "settings":
      return (
        <SettingsSelector
          store={store}
          settingsManager={props.options?.settingsManager}
          controller={ctrl}
          surfaceId={surfaceId}
          onClose={() => ctrl.closeSurface(surface.id)}
        />
      );

    case "login":
      return (
        <LoginDialog
          store={store}
          provider={(data?.provider as string) ?? store.state().model.current.provider}
          onClose={() => ctrl.closeSurface(surface.id)}
        />
      );

    case "notifications": {
      const notifs = store.state().notifications;
      const items = notifs.map((n) => ({
        id: n.id,
        label: n.message,
        description: `${n.severity} — ${n.source}`,
        value: n,
        badge: n.readAt ? undefined : "new",
      }));
      return (
        <ReadOnlyListSurface
          title="Notifications"
          items={items}
          hints={["↑↓ navigate  Esc close  Enter mark read"]}
          surfaceId={surfaceId}
          controller={ctrl}
          onClose={() => ctrl.closeSurface(surface.id)}
          onConfirm={(item) => ctrl.notifications.markRead(item.value.id)}
        />
      );
    }

    case "hotkeys": {
      const bindings = ctrl.keymap.listBindings();
      const items = bindings.map((b) => ({
        id: b.id,
        label: b.id,
        description: ctrl.keymap.keyDisplayText(b.id),
        value: b,
      }));
      return (
        <ReadOnlyListSurface
          title="Keybindings"
          items={items}
          hints={["↑↓ navigate  Esc close"]}
          surfaceId={surfaceId}
          controller={ctrl}
          onClose={() => ctrl.closeSurface(surface.id)}
          onConfirm={() => {}}
        />
      );
    }

    case "help": {
      const cmds = ctrl.commands.listSlashCommands();
      const items = cmds.map((c) => ({
        id: c.name,
        label: c.name,
        description: c.description,
        value: c,
      }));
      return (
        <ReadOnlyListSurface
          title="Available Commands"
          items={items}
          hints={["↑↓ navigate  Esc close"]}
          surfaceId={surfaceId}
          controller={ctrl}
          onClose={() => ctrl.closeSurface(surface.id)}
          onConfirm={() => {}}
        />
      );
    }

    default:
      return (
        <box padding={1}>
          <text>Unknown surface: {surfaceType ?? surface.role}</text>
        </box>
      );
  }
}

/**
 * Reusable surface component for read-only browseable lists
 * (notifications, hotkeys, help). Registers keyboard handling for
 * arrow navigation + Esc close through the surface controller.
 */
function ReadOnlyListSurface(props: {
  title: string;
  items: SelectItem<any>[];
  hints: string[];
  surfaceId: string;
  controller: TuiController;
  onClose: () => void;
  onConfirm: (item: SelectItem<any>) => void;
}) {
  const [listState, setListState] = createSignal<SelectableListState>(
    createSelectableListState(),
  );

  onMount(() => {
    props.controller.setSurfaceController(props.surfaceId, {
      handleKey(event: FocusKeyEvent): boolean {
        const next = handleSelectableListKey(listState(), event, {
          total: props.items.length,
        });
        if (next) {
          setListState(next);
          return true;
        }
        if (event.name === "enter" || event.name === "return") {
          const item = getSelectedItem(props.items, listState().selectedIndex);
          if (item) props.onConfirm(item);
          return true;
        }
        if (event.name === "escape") {
          props.onClose();
          return true;
        }
        return false;
      },
    });
  });
  onCleanup(() => props.controller.setSurfaceController(props.surfaceId, null));

  return (
    <SelectorShell title={props.title} onClose={props.onClose} hints={props.hints}>
      <SelectListView
        items={props.items}
        selectedIndex={listState().selectedIndex}
        showDescriptions
        maxHeight={12}
        onSelect={() => {}}
      />
    </SelectorShell>
  );
}
