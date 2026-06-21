// ============================================================================
// Auth Type Selector — first step of /login: choose OAuth vs API Key
//
// Self-contained: owns all state, keyboard handling, and UI composition.
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { KeyEvent } from "../../../focus/types.js";
import type { PanelRuntime } from "../../../panels/panel-runtime.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import { type SurfaceKeyResult, selectorBehavior } from "../../../surfaces/index.js";
import {
  createSelectableListState,
  getSelectedItem,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import type { ActionService } from "../action-service.js";
import { ListBody } from "../primitives/index.js";
import type { SelectItem } from "./selector-controller.js";

const AUTH_TYPE_OPTIONS = [
  {
    id: "oauth",
    label: "OAuth / Subscription Login",
    description: "Sign in via browser (Anthropic, OpenAI Codex, GitHub Copilot, etc.)",
  },
  {
    id: "api_key",
    label: "API Key",
    description: "Enter an API key directly (OpenAI, Google, DeepSeek, etc.)",
  },
];

export interface AuthTypeSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  runtime: PanelRuntime;
  availableWidth: number;
  availableHeight: number;
  onClose: () => void;
}

export function AuthTypeSelector(props: AuthTypeSelectorProps) {
  const { controller, surfaceId, runtime, availableWidth, availableHeight } = props;

  const [listState, setListState] = createSignal<SelectableListState>(createSelectableListState());

  const items = createMemo<SelectItem<string>[]>(() =>
    AUTH_TYPE_OPTIONS.map((opt) => ({
      id: opt.id,
      label: opt.label,
      description: opt.description,
      value: opt.id,
    })),
  );

  function confirm(): void {
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (!item) return;

    if (item.value === "oauth") {
      runtime.dispatch({
        type: "push_route",
        route: {
          id: "login.oauth-provider-picker",
          chrome: {
            title: "OAuth Login",
            hints: ["Up/Down move  Enter select  Esc back"],
          },
          interaction: "list",
          capabilities: [{ kind: "list", selectable: true }],
          body: {
            type: "provider-picker",
            payload: { mode: "oauth" },
          },
        },
      });
    } else {
      runtime.dispatch({
        type: "push_route",
        route: {
          id: "login.api-key-provider-picker",
          chrome: {
            title: "API Key Login",
            hints: ["Up/Down move  Enter select  Esc back"],
          },
          interaction: "list",
          capabilities: [{ kind: "list", selectable: true }],
          body: {
            type: "provider-picker",
            payload: { mode: "api_key" },
          },
        },
      });
    }
  }

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): SurfaceKeyResult {
        const { nextState, result } = selectorBehavior(event, listState(), items().length);
        setListState(nextState);
        return result;
      },
      onConfirm() {
        confirm();
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  const listMaxH = () => availableHeight;

  return (
    <box flexDirection="column">
      <ListBody
        items={items()}
        selectedIndex={listState().selectedIndex}
        maxHeight={listMaxH()}
        width={availableWidth}
        showDescriptions={true}
      />
    </box>
  );
}
