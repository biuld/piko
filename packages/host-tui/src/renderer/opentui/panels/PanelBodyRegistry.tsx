import { createSignal, onCleanup, onMount } from "solid-js";
import type { PikoHost } from "piko-host-runtime";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { TuiStore } from "../store.js";
import type { ActionService } from "../action-service.js";
import type { PanelBody } from "../../../panels/types.js";
import type { PanelRuntime } from "../../../panels/panel-runtime.js";
import { ModelSelector } from "../select/ModelSelector.js";
import { ThinkingSelector } from "../select/ThinkingSelector.js";
import { ResumeSelector } from "../select/ResumeSelector.js";
import { SettingsSelector } from "../select/SettingsSelector.js";
import { TextInputBody } from "./TextInputBody.js";
import { SelectListView } from "../select/SelectListView.js";
import type { SelectItem } from "../select/selector-controller.js";

export interface PanelBodyRegistryProps {
  surfaceId: string;
  body: PanelBody<any>;
  runtime: PanelRuntime;
  store: TuiStore;
  controller: TuiController;
  actionSvc: ActionService;
  host: PikoHost;
  settingsManager?: any;
}

export function PanelBodyRegistry(props: PanelBodyRegistryProps) {
  const { surfaceId, body, runtime, store, controller: ctrl, actionSvc, host, settingsManager } = props;

  switch (body.type) {
    case "model-picker":
      return (
        <ModelSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          initialQuery={runtime.state.filterText as string | undefined}
          onQueryChange={(query) => runtime.dispatch({ type: "update_filter", text: query })}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "thinking-picker":
      return (
        <ThinkingSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "session-resume":
      return (
        <ResumeSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          initialQuery={runtime.state.filterText as string | undefined}
          onQueryChange={(query) => runtime.dispatch({ type: "update_filter", text: query })}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "settings":
      return (
        <SettingsSelector
          store={store}
          settingsManager={settingsManager}
          controller={ctrl}
          surfaceId={surfaceId}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "login":
      return (
        <TextInputBody
          label={`Enter API key for ${body.payload?.provider || "provider"}:`}
          placeholder="sk-..."
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
          onConfirm={(val) => {
            // API key storage is handled by the host/auth layer.
            ctrl.notifications.notify({
              message: "Login logic not fully wired in UI yet, use piko login <provider> <key>",
              severity: "warning",
            });
          }}
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
        <ReadOnlyListBody
          items={items}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
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
        <ReadOnlyListBody
          items={items}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
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
        <ReadOnlyListBody
          items={items}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
          onConfirm={() => {}}
        />
      );
    }

    case "changelog":
      return (
        <ReadOnlyListBody
          items={[
            { id: "v1", label: "piko v1", description: "TUI + Engine architecture", value: null },
          ]}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
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
        <ReadOnlyListBody
          items={items}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
          onConfirm={() => {}}
        />
      );
    }

    case "session-fork": {
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
        <ReadOnlyListBody
          items={entries()}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
          onConfirm={(item) => {
            const h = host as any;
            if (h?.forkSession && item.value?.id) {
              h.forkSession(item.value.id)
                .then(() => {
                  ctrl.notifications.notify({
                    message: `Forked at message ${item.label}`,
                    severity: "success",
                  });
                  runtime.dispatch({ type: "cancel" });
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
        <ReadOnlyListBody
          items={treeItems()}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
          onConfirm={(item) => {
            const sessionId = item.value?.id ?? item.value?.leafId;
            if (sessionId) {
              runtime.dispatch({ type: "cancel" });
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

    case "session-import": {
      return (
        <TextInputBody
          label="Path:"
          placeholder="(type JSONL file path...)"
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
          onConfirm={async (val) => {
            try {
              const h = host as any;
              if (h.importSession) {
                await h.importSession(val);
                ctrl.notifications.notify({ message: "Session imported", severity: "success" });
              }
            } catch(e: any) {
              ctrl.notifications.notify({ message: `Import failed: ${e.message}`, severity: "error" });
            }
          }}
        />
      );
    }

    case "session-rename": {
      return (
        <TextInputBody
          label="Name:"
          placeholder="(type a name...)"
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
          onConfirm={async (val) => {
            try {
              const sessionId = store.state().session.sessionId;
              if (sessionId) {
                await actionSvc.host.renameSession(sessionId, val);
                ctrl.notifications.notify({ message: `Session renamed to ${val}`, severity: "success" });
              }
            } catch(e: any) {
              ctrl.notifications.notify({ message: `Rename failed: ${e.message}`, severity: "error" });
            }
          }}
        />
      );
    }

    default:
      return (
        <box padding={1}>
          <text>Unknown panel body: {body.type}</text>
        </box>
      );
  }
}

export function ReadOnlyListBody(props: {
  items: SelectItem<any>[];
  runtime: PanelRuntime;
  controller: TuiController;
  surfaceId: string;
  width: number;
  maxHeight?: number;
  onConfirm: (item: SelectItem<any>) => void;
}) {
  const surface = () => props.controller.store.state().surfaces.find((s) => s.id === props.surfaceId);
  const placement = () => surface()?.placement ?? "partial";
  const viewportHeight = () => props.controller.store.state().layout.viewport.height;

  const maxHeight = () => {
    if (props.maxHeight !== undefined) return props.maxHeight;
    if (placement() === "full") {
      return Math.max(15, viewportHeight() - 6);
    }
    let reserved = 0;
    const route = props.runtime.currentRoute;
    if (route.chrome.hints && route.chrome.hints.length > 0) {
      reserved += 1;
    }
    if (route.capabilities.some(c => c.kind === "filter")) {
      reserved += 2;
    }
    return 12 - reserved;
  };

  onMount(() => {
    props.controller.setSurfaceController(props.surfaceId, {
      handleKey(event) {
        const current = props.runtime.state.selectedIndex ?? 0;
        let next = current;
        if (event.name === "up") next = Math.max(0, current - 1);
        else if (event.name === "down") next = Math.min(props.items.length - 1, current + 1);
        else if (event.name === "pageup") next = Math.max(0, current - 10);
        else if (event.name === "pagedown") next = Math.min(props.items.length - 1, current + 10);
        else if (event.name === "home") next = 0;
        else if (event.name === "end") next = Math.max(0, props.items.length - 1);

        if (next !== current) {
          props.runtime.dispatch({ type: "update_selection", index: next });
          return { type: "handled" };
        }
        if (event.name === "enter" || event.name === "return") {
          return { type: "confirm" };
        }
        return { type: "unhandled" };
      },
      onConfirm() {
        const item = props.items[props.runtime.state.selectedIndex ?? 0];
        if (item) {
          props.onConfirm(item);
        }
        props.runtime.dispatch({ type: "cancel" });
      }
    });
  });

  onCleanup(() => props.controller.setSurfaceController(props.surfaceId, null));

  return (
    <box flexDirection="column">
      <SelectListView
        items={props.items}
        selectedIndex={props.runtime.state.selectedIndex ?? 0}
        width={props.width}
        maxHeight={maxHeight()}
        showDescriptions
        onSelect={() => {}}
      />
    </box>
  );
}
