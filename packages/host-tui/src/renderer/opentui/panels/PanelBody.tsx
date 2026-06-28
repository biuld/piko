import { createSignal, onMount } from "solid-js";
import type { TuiFlatTreeEntry } from "../../../app/tui-host.js";
import type { TuiPreferences } from "../../../app/tui-preferences.js";
import type { PanelRuntime } from "../../../panels/panel-runtime.js";
import type { PanelBody as PanelBodyType } from "../../../panels/types.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import { flattenSessionTree } from "../../../shared/index.js";
import type { ActionService } from "../action-service.js";
import { ReadOnlyList, TextInput } from "../primitives/index.js";
import { AuthTypeSelector } from "../select/AuthTypeSelector.js";
import { ModelSelector } from "../select/ModelSelector.js";
import { OAuthLoginFlow } from "../select/OAuthLoginFlow.js";
import { ProviderSelector } from "../select/ProviderSelector.js";
import { ResumeSelector } from "../select/ResumeSelector.js";
import { SettingsSelector } from "../select/SettingsSelector.js";
import { ThinkingSelector } from "../select/ThinkingSelector.js";
import { TreeSelector } from "../select/TreeSelector.js";
import type { TuiStore } from "../store.js";
import { ToolApprovalBody } from "./ToolApprovalBody.js";

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
  preferences?: TuiPreferences;
  availableHeight: number;
  availableWidth: number;
}

export function PanelBody(props: PanelBodyProps) {
  const { surfaceId, body, runtime, store, controller: ctrl, actionSvc, preferences } = props;

  switch (body.type) {
    case "tool-approval":
      return <ToolApprovalBody store={store} />;

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
          preferences={preferences}
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "auth-type-picker":
      return (
        <AuthTypeSelector
          actionSvc={actionSvc}
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
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
          mode={(body.payload as any)?.mode ?? "api_key"}
          availableWidth={props.availableWidth}
          availableHeight={props.availableHeight}
          onClose={() => runtime.dispatch({ type: "cancel" })}
        />
      );

    case "login": {
      const loginPayload = body.payload as { provider?: string; mode?: "oauth" | "api_key" };
      if (loginPayload.mode === "oauth") {
        return (
          <OAuthLoginFlow
            provider={loginPayload.provider || ""}
            providerName={loginPayload.provider || ""}
            actionSvc={actionSvc}
            surfaceId={surfaceId}
            onComplete={(success, message) => {
              if (success) {
                ctrl.notifications.notify({
                  message: message ?? `Logged in to ${loginPayload.provider}`,
                  severity: "success",
                });
              } else if (message) {
                ctrl.notifications.notify({
                  message: `Login failed: ${message}`,
                  severity: "error",
                });
              }
              runtime.dispatch({ type: "cancel" });
            }}
          />
        );
      }
      return (
        <TextInput
          label={`Enter API key for ${loginPayload.provider || "provider"}:`}
          placeholder="sk-..."
          controller={ctrl}
          surfaceId={surfaceId}
          runtime={runtime}
          onConfirm={(val) => {
            const provider = loginPayload.provider;
            if (provider && val.trim()) {
              actionSvc.setApiKey(provider, val.trim());
              ctrl.notifications.notify({
                message: `API key submitted for ${provider}`,
                severity: "info",
              });
            }
            runtime.dispatch({ type: "cancel" });
          }}
        />
      );
    }

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
        // Read entries from store (populated by hostd snapshot).
        const treeEntries = store.state().session.entries;
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
      const [treeEntries, setTreeEntries] = createSignal<TuiFlatTreeEntry[]>([]);
      const [leafId, setLeafId] = createSignal<string | null>(null);
      const [loading, _setLoading] = createSignal(false);

      // Read entries directly from store (populated by hostd snapshot events).
      onMount(() => {
        const state = store.state();
        const rawEntries = state.session.entries;
        const currentLeafId = state.session.currentLeafId;
        if (rawEntries.length > 0) {
          const { flat } = flattenSessionTree(rawEntries, currentLeafId);
          setTreeEntries(flat);
          setLeafId(currentLeafId);
        }
        // If no entries yet, the panel will show empty — entries arrive via
        // session_opened/state_snapshot hostd events which trigger re-render.
      });

      return (
        <TreeSelector
          entries={treeEntries()}
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
