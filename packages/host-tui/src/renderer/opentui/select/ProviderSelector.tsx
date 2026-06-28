// ============================================================================
// Provider Selector — ListBody + HintBar.
//
// Self-contained: owns all state, keyboard handling, and UI composition.
// Supports two modes:
//   - "oauth": show OAuth-capable providers (from getOAuthProviders)
//   - "api_key": show API key providers (hardcoded list, minus OAuth providers)
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

const OAUTH_PROVIDER_IDS = new Set(["openai"]);

export interface ProviderSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  runtime: PanelRuntime;
  /** "oauth" shows OAuth providers only; "api_key" shows API key providers only. */
  mode?: "oauth" | "api_key";
  availableWidth: number;
  availableHeight: number;
  onClose: () => void;
}

export function ProviderSelector(props: ProviderSelectorProps) {
  const {
    actionSvc,
    controller,
    surfaceId,
    runtime,
    mode = "api_key",
    availableWidth,
    availableHeight,
  } = props;

  const [listState, setListState] = createSignal<SelectableListState>(createSelectableListState());

  const oauthProviderIds = createMemo(() => {
    if (mode === "oauth") return new Set<string>();
    return OAUTH_PROVIDER_IDS;
  });

  const items = createMemo<SelectItem<string>[]>(() => {
    // Get provider list from hostd model catalog (dynamic, not hardcoded)
    const state = actionSvc.getState();
    const catalog = state.model.modelCatalog || [];

    if (mode === "oauth") {
      // Only show providers that are in the catalog AND support OAuth
      return catalog
        .filter((p) => OAUTH_PROVIDER_IDS.has(p.provider))
        .map((p) => ({
          id: p.provider,
          label: p.provider,
          description: p.hasAuth ? "OAuth / Subscription [✓]" : "OAuth / Subscription",
          value: p.provider,
        }));
    }

    // API key mode — all providers from catalog, minus OAuth-only ones
    const excluded = oauthProviderIds();
    return catalog
      .filter((p) => !excluded.has(p.provider))
      .map((p) => ({
        id: p.provider,
        label: p.provider,
        description: p.hasAuth ? `${p.models.length} model(s) [✓]` : `${p.models.length} model(s)`,
        value: p.provider,
      }));
  });

  function confirm(): void {
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (!item) return;

    if (mode === "oauth") {
      // Push the login.oauth-form route
      runtime.dispatch({
        type: "push_route",
        route: {
          id: "login.oauth-form",
          chrome: {
            title: `Login - ${item.label}`,
            hints: ["Enter submit  Esc back"],
          },
          interaction: "form",
          capabilities: [],
          body: {
            type: "login",
            payload: { provider: item.value, mode: "oauth" },
          },
        },
      });
    } else {
      // Push the login.form route (API key input)
      runtime.dispatch({
        type: "push_route",
        route: {
          id: "login.form",
          chrome: {
            title: `Login - ${item.label}`,
            hints: ["Enter submit  Esc back"],
          },
          interaction: "form",
          capabilities: [],
          body: {
            type: "login",
            payload: { provider: item.value, mode: "api_key" },
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
