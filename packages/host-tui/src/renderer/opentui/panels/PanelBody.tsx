import type { FlatTreeEntry, PikoHost } from "piko-host-runtime";
import { flattenSessionTree } from "piko-host-runtime";
import { createSignal, onMount } from "solid-js";
import type { PanelRuntime } from "../../../panels/panel-runtime.js";
import type { PanelBody as PanelBodyType } from "../../../panels/types.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { ActionService } from "../action-service.js";
import { ReadOnlyList, TextInput } from "../primitives/index.js";
import { ModelSelector } from "../select/ModelSelector.js";
import { ProviderSelector } from "../select/ProviderSelector.js";
import { ResumeSelector } from "../select/ResumeSelector.js";
import { SettingsSelector } from "../select/SettingsSelector.js";
import { ThinkingSelector } from "../select/ThinkingSelector.js";
import { TreeSelector } from "../select/TreeSelector.js";
import type { TuiStore } from "../store.js";

// ============================================================================
// Helpers
// ============================================================================

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

// ============================================================================
// PanelBody
// ============================================================================

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

    case "provider-picker":
      return (
        <ProviderSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "login":
      return (
        <TextInput
          label={`Enter API key for ${body.payload?.provider || "provider"}:`}
          placeholder="sk-..."
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
          onConfirm={(_val) => {
            const provider = body.payload?.provider;
            if (provider && actionSvc.modelRegistry) {
              try {
                const authStorage = actionSvc.modelRegistry.getAuthStorage();
                authStorage.set(provider, { type: "api_key", key: _val });
                ctrl.notifications.notify({
                  message: `Successfully saved API key for ${provider}`,
                  severity: "success",
                });
                runtime.dispatch({ type: "cancel" });
              } catch (e: any) {
                ctrl.notifications.notify({
                  message: `Failed to save API key: ${e.message}`,
                  severity: "error",
                });
              }
            } else {
              ctrl.notifications.notify({
                message: "Could not save API key: provider or model registry is missing.",
                severity: "warning",
              });
            }
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
        <ReadOnlyList
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
        <ReadOnlyList
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
        <ReadOnlyList
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
        <ReadOnlyList
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
        <ReadOnlyList
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
        <ReadOnlyList
          items={entries()}
          runtime={runtime}
          controller={ctrl}
          surfaceId={surfaceId}
          width={ctrl.store.state().layout.viewport.width}
          itemSpacing={1}
          onConfirm={async (item) => {
            if (item.value?.id) {
              await actionSvc.session.forkSession(item.value.id, surfaceId);
            }
          }}
        />
      );
    }

    case "session-tree": {
      const [entries, setEntries] = createSignal<FlatTreeEntry[]>([]);
      const [leafId, setLeafId] = createSignal<string | null>(null);
      const [loading, setLoading] = createSignal(true);

      onMount(() => {
        const leafPromise = host.getLeafId();
        Promise.all([Promise.resolve(leafPromise), host.getTreeEntries()])
          .then(([lId, rawEntries]) => {
            const { flat } = flattenSessionTree(rawEntries, lId ?? null);
            setEntries(flat);
            setLeafId(lId ?? null);
          })
          .catch(() => {})
          .finally(() => setLoading(false));
      });

      return (
        <TreeSelector
          entries={entries()}
          leafId={leafId()}
          loading={loading()}
          onSelect={async (entryId) => {
            await actionSvc.session.navigateTree(entryId, surfaceId);
          }}
          onCancel={() => runtime.dispatch({ type: "cancel" })}
          controller={ctrl}
          surfaceId={surfaceId}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
          initialQuery={runtime.state.filterText as string | undefined}
        />
      );
    }

    case "session-import": {
      return (
        <TextInput
          label="Path:"
          placeholder="(type JSONL file path...)"
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
          onConfirm={async (val) => {
            await actionSvc.session.importSession(val, surfaceId);
          }}
        />
      );
    }

    case "session-rename": {
      return (
        <TextInput
          label="Name:"
          placeholder="(type a name...)"
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
          onConfirm={async (val) => {
            const sessionId = store.state().session.sessionId;
            await actionSvc.session.renameSession(val, sessionId, surfaceId);
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
