import type { PikoHost } from "piko-host-runtime";
import { createSignal, onCleanup, onMount } from "solid-js";
import type { PanelRuntime } from "../../../panels/panel-runtime.js";
import type { PanelBody as PanelBodyType } from "../../../panels/types.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { ActionService } from "../action-service.js";
import { ModelSelector } from "../select/ModelSelector.js";
import { ResumeSelector } from "../select/ResumeSelector.js";
import { SelectListView } from "../select/SelectListView.js";
import { SettingsSelector } from "../select/SettingsSelector.js";
import type { SelectItem } from "../select/selector-controller.js";
import { ThinkingSelector } from "../select/ThinkingSelector.js";
import { TreeSelector } from "../select/TreeSelector.js";
import type { TuiStore } from "../store.js";
import { TextInputBody } from "./TextInputBody.js";

function extractUserMessageText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .filter(
        (part): part is { type: string; text?: string } =>
          typeof part === "object" &&
          part !== null &&
          "type" in part &&
          (part as { type?: unknown }).type === "text",
      )
      .map((part) => part.text ?? "")
      .join("\n");
  }
  return "";
}

function normalizeListText(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

export interface PanelBodyProps {
  surfaceId: string;
  body: PanelBodyType<any>;
  runtime: PanelRuntime;
  store: TuiStore;
  controller: TuiController;
  actionSvc: ActionService;
  host: PikoHost;
  settingsManager?: any;
  availableHeight: number;
  availableWidth: number;
}

export function PanelBody(props: PanelBodyProps) {
  const {
    surfaceId,
    body,
    runtime,
    store,
    controller: ctrl,
    actionSvc,
    host,
    settingsManager,
  } = props;

  switch (body.type) {
    case "model-picker":
      return (
        <ModelSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          initialQuery={runtime.state.filterText as string | undefined}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "thinking-picker":
      return (
        <ThinkingSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "session-resume":
      return (
        <ResumeSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
          initialQuery={runtime.state.filterText as string | undefined}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "settings":
      return (
        <SettingsSelector
          settingsManager={settingsManager}
          host={host}
          controller={ctrl}
          surfaceId={surfaceId}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
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
          onConfirm={(_val) => {
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
        Array<{ id: string; label: string; meta: string; value: any }>
      >([]);
      onMount(() => {
        host
          .getTreeEntries()
          .then((treeEntries: any[]) => {
            const userMessages = treeEntries
              .filter((entry: any) => entry.type === "message" && entry.message?.role === "user")
              .map((entry: any) => ({
                entry,
                text: normalizeListText(extractUserMessageText(entry.message?.content)),
              }))
              .filter((item) => item.text.length > 0);

            setEntries(
              userMessages.map(({ entry, text }, i: number) => ({
                id: entry.id,
                label: text,
                meta: `Message ${i + 1} of ${userMessages.length}`,
                value: entry,
              })),
            );
          })
          .catch(() => setEntries([]));
      });
      return (
        <ReadOnlyListBody
          items={entries()}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
          itemSpacing={1}
          onConfirm={async (item) => {
            if (item.value?.id) {
              try {
                const result = await host.forkSession(item.value.id);

                // Reset TUI state to reflect the forked session
                const sessionId = host.sessionId;
                const sessionName = await host.getSessionName();
                const entries = await host.loadBranchEntries();
                const { entriesToTranscript } = await import(
                  "../../../timeline/entries-to-transcript.js"
                );
                const transcript = entriesToTranscript(entries);

                store.dispatch({
                  type: "session_resumed",
                  sessionId,
                  sessionName: sessionName ?? undefined,
                  transcript,
                });

                ctrl.notifications.notify({
                  message: "Forked to new session",
                  severity: "success",
                });
                if (result.selectedText) {
                  ctrl.setEditorText(result.selectedText);
                }
              } catch (e: any) {
                ctrl.notifications.notify({
                  message: `Fork failed: ${e.message}`,
                  severity: "error",
                });
              }
            }
          }}
        />
      );
    }

    case "session-tree":
      return (
        <TreeSelector
          actionSvc={actionSvc}
          controller={ctrl}
          host={host}
          surfaceId={surfaceId}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
          initialQuery={runtime.state.filterText as string | undefined}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

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
              await host.importSession(val);

              // Reset TUI state to reflect the imported session
              const sessionId = host.sessionId;
              const sessionName = await host.getSessionName();
              const entries = await host.loadBranchEntries();
              const { entriesToTranscript } = await import(
                "../../../timeline/entries-to-transcript.js"
              );
              const transcript = entriesToTranscript(entries);

              store.dispatch({
                type: "session_resumed",
                sessionId,
                sessionName: sessionName ?? undefined,
                transcript,
              });

              ctrl.notifications.notify({ message: "Session imported", severity: "success" });
            } catch (e: any) {
              ctrl.notifications.notify({
                message: `Import failed: ${e.message}`,
                severity: "error",
              });
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
                ctrl.notifications.notify({
                  message: `Session renamed to ${val}`,
                  severity: "success",
                });
              }
            } catch (e: any) {
              ctrl.notifications.notify({
                message: `Rename failed: ${e.message}`,
                severity: "error",
              });
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
  itemSpacing?: number;
  onConfirm: (item: SelectItem<any>) => void | Promise<void>;
}) {
  const surface = () =>
    props.controller.store.state().surfaces.find((s) => s.id === props.surfaceId);
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
    if (route.capabilities.some((c) => c.kind === "filter")) {
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
      async onConfirm() {
        const item = props.items[props.runtime.state.selectedIndex ?? 0];
        if (item) {
          await props.onConfirm(item);
        }
        props.runtime.dispatch({ type: "cancel" });
      },
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
        itemSpacing={props.itemSpacing ?? 0}
        onSelect={() => {}}
      />
    </box>
  );
}
