// ============================================================================
// SurfaceContentRegistry — maps surface data.type to renderer components.
// Extracted from App.tsx so render logic is separate from the app shell.
// ============================================================================

import { createSignal, onCleanup, onMount } from "solid-js";
import type { PikoHost } from "piko-host-runtime";
import type { TuiState } from "../../../state/state.js";
import type { KeyEvent as FocusKeyEvent } from "../../../focus/types.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { TuiSurfaceState } from "../../../surfaces/types.js";
import {
  createSelectableListState,
  getSelectedItem,
  handleSelectableListKey,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import type { SelectItem } from "../select/selector-controller.js";
import { SelectorShell } from "../select/SelectorShell.js";
import { SelectListView } from "../select/SelectListView.js";
import { ModelSelector } from "../select/ModelSelector.js";
import { ThinkingSelector } from "../select/ThinkingSelector.js";
import { ResumeSelector } from "../select/ResumeSelector.js";
import { SettingsSelector } from "../select/SettingsSelector.js";
import { LoginDialog } from "../LoginDialog.js";
import type { ActionService } from "../action-service.js";
import type { TuiStore } from "../store.js";

export interface SurfaceContentRegistryProps {
  surface: TuiSurfaceState;
  store: TuiStore;
  controller: TuiController;
  actionSvc: ActionService;
  host: PikoHost;
  settingsManager?: any;
}

export function SurfaceContentRegistry(props: SurfaceContentRegistryProps) {
  const { surface, store, controller: ctrl, actionSvc, host, settingsManager } = props;
  const data = surface.data as Record<string, unknown> | undefined;
  const surfaceType = data?.type as string | undefined;
  const surfaceId = surface.id;

  // Hints generated from keymap (shared across multiple surface types)
  const browseHints = [
    ctrl.keymap.formatHintLine([
      ["tui.select.up", "navigate"],
      ["tui.select.down", ""],
      ["tui.select.cancel", "close"],
    ]),
  ];
  const notifHints = [
    ctrl.keymap.formatHintLine([
      ["tui.select.up", "navigate"],
      ["tui.select.down", ""],
      ["tui.select.cancel", "close"],
      ["tui.select.confirm", "mark read"],
    ]),
  ];

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
          settingsManager={settingsManager}
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
          hints={notifHints}
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
          hints={browseHints}
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
          hints={browseHints}
          surfaceId={surfaceId}
          controller={ctrl}
          onClose={() => ctrl.closeSurface(surface.id)}
          onConfirm={() => {}}
        />
      );
    }

    case "changelog":
      return (
        <ReadOnlyListSurface
          title="Changelog"
          items={[
            { id: "v1", label: "piko v1", description: "TUI + Engine architecture, surface system, focus tree, timeline stable IDs", value: null },
          ]}
          hints={browseHints}
          surfaceId={surfaceId}
          controller={ctrl}
          onClose={() => ctrl.closeSurface(surface.id)}
          onConfirm={() => {}}
        />
      );

    case "session-info": {
      const s = store.state().session;
      const items = [
        { id: "id", label: "Session ID", description: s.sessionId ?? "(new)", value: null },
        { id: "name", label: "Name", description: s.sessionName ?? "(unnamed)", value: null },
        { id: "cwd", label: "Directory", description: s.cwd, value: null },
        { id: "messages", label: "Messages", description: String(s.messageCount), value: null },
        { id: "git", label: "Git branch", description: s.gitBranch ?? "(none)", value: null },
      ];
      return (
        <ReadOnlyListSurface
          title="Session Info"
          items={items}
          hints={browseHints}
          surfaceId={surfaceId}
          controller={ctrl}
          onClose={() => ctrl.closeSurface(surface.id)}
          onConfirm={() => {}}
        />
      );
    }

    case "fork-session": {
      const [entries, setEntries] = createSignal<
        Array<{ id: string; label: string; description: string; value: any }>
      >([]);
      onMount(() => {
        const h = host as any;
        if (h?.getBranchEntries) {
          h.getBranchEntries()
            .then((branch: any[]) => {
              setEntries(
                branch.map((e: any, i: number) => ({
                  id: e.id,
                  label: `[${i}] ${(e.summary ?? e.text ?? "").slice(0, 60)}`,
                  description: e.role ?? "message",
                  value: e,
                })),
              );
            })
            .catch(() => setEntries([]));
        }
      });
      return (
        <ReadOnlyListSurface
          title="Fork Session — select message"
          items={entries()}
          hints={browseHints}
          surfaceId={surfaceId}
          controller={ctrl}
          onClose={() => ctrl.closeSurface(surface.id)}
          onConfirm={(item) => {
            const h = host as any;
            if (h?.forkSession && item.value?.id) {
              h.forkSession(item.value.id)
                .then(() => {
                  ctrl.notifications.notify({
                    message: `Forked at message ${item.label}`,
                    severity: "success",
                  });
                  ctrl.closeSurface(surface.id);
                })
                .catch((e: any) => {
                  ctrl.notifications.notify({
                    message: `Fork failed: ${e.message}`,
                    severity: "error",
                  });
                });
            }
          }}
        />
      );
    }

    case "session-tree": {
      const [treeItems, setTreeItems] = createSignal<
        Array<{ id: string; label: string; description: string; value: any }>
      >([]);
      onMount(() => {
        const h = host as any;
        if (h?.listSessions) {
          h.listSessions({ scope: "current" })
            .then((sessions: any[]) => {
              setTreeItems(
                sessions.map((s: any) => ({
                  id: s.id,
                  label: s.name ?? s.id.slice(0, 12),
                  description: `${s.messageCount ?? "?"} messages`,
                  value: s,
                })),
              );
            })
            .catch(() => {
              setTreeItems([]);
            });
        }
      });
      return (
        <ReadOnlyListSurface
          title="Session Tree"
          items={treeItems()}
          hints={browseHints}
          surfaceId={surfaceId}
          controller={ctrl}
          onClose={() => ctrl.closeSurface(surface.id)}
          onConfirm={(item) => {
            const sessionId = item.value?.id ?? item.value?.leafId;
            if (sessionId) {
              ctrl.closeSurface(surface.id);
              actionSvc.switchSession(sessionId).catch((e: any) => {
                ctrl.notifications.notify({
                  message: `Failed to switch session: ${e.message}`,
                  severity: "error",
                });
              });
            }
          }}
        />
      );
    }

    case "import-session": {
      const [path, setPath] = createSignal("");
      const handleSubmit = () => {
        const value = path().trim();
        if (!value) return;
        const h = host as any;
        if (h?.importSession) {
          h.importSession(value)
            .then(() => {
              ctrl.notifications.notify({
                message: `Imported session from ${value}`,
                severity: "success",
              });
              ctrl.closeSurface(surface.id);
            })
            .catch((e: any) => {
              ctrl.notifications.notify({
                message: `Import failed: ${e.message}`,
                severity: "error",
              });
            });
        }
      };
      onMount(() => {
        ctrl.setSurfaceController(surfaceId, {
          handleKey(event: FocusKeyEvent): boolean {
            if (event.char && event.char >= " ") {
              setPath((p) => p + event.char!);
              return true;
            }
            if (event.name === "backspace") {
              setPath((p) => p.slice(0, -1));
              return true;
            }
            if (event.name === "enter" || event.name === "return") {
              handleSubmit();
              return true;
            }
            if (event.name === "escape") {
              ctrl.closeSurface(surface.id);
              return true;
            }
            return false;
          },
        });
      });
      onCleanup(() => ctrl.setSurfaceController(surfaceId, null));
      return (
        <SelectorShell
          title="Import Session"
          onClose={() => ctrl.closeSurface(surface.id)}
          hints={["Type path  Enter import  Esc cancel"]}
        >
          <box padding={1}>
            <text>Path: {path() || "(type JSONL file path...)"}</text>
          </box>
        </SelectorShell>
      );
    }

    case "rename-session": {
      const [name, setName] = createSignal("");
      const handleSubmit = () => {
        const value = name().trim();
        if (!value) return;
        const h = host as any;
        if (h?.setSessionName) {
          h.setSessionName(value).then(() => {
            ctrl.notifications.notify({
              message: `Session renamed to "${value}"`,
              severity: "success",
            });
            ctrl.closeSurface(surface.id);
          }).catch((e: any) => {
            ctrl.notifications.notify({
              message: `Rename failed: ${e.message}`,
              severity: "error",
            });
          });
        }
      };
      onMount(() => {
        ctrl.setSurfaceController(surfaceId, {
          handleKey(event: FocusKeyEvent): boolean {
            if (event.char && event.char >= " ") {
              setName((n) => n + event.char!);
              return true;
            }
            if (event.name === "backspace") {
              setName((n) => n.slice(0, -1));
              return true;
            }
            if (event.name === "enter" || event.name === "return") {
              handleSubmit();
              return true;
            }
            if (event.name === "escape") {
              ctrl.closeSurface(surface.id);
              return true;
            }
            return false;
          },
        });
      });
      onCleanup(() => ctrl.setSurfaceController(surfaceId, null));
      return (
        <SelectorShell
          title="Rename Session"
          onClose={() => ctrl.closeSurface(surface.id)}
          hints={["Type name  Enter confirm  Esc cancel"]}
        >
          <box padding={1}>
            <text>{name() || "(type a name...)"}</text>
          </box>
        </SelectorShell>
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
 * (notifications, hotkeys, help, session tree). Registers keyboard handling
 * through the surface controller.
 */
export function ReadOnlyListSurface(props: {
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
        width={props.controller.store.state().layout.viewport.width}
        showDescriptions
        maxHeight={12}
        onSelect={() => {}}
      />
    </SelectorShell>
  );
}
