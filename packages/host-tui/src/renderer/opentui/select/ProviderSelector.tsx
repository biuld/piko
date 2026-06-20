// ============================================================================
// Provider Selector — ListBody + HintBar.
//
// Self-contained: owns all state, keyboard handling, and UI composition.
// Supports two modes:
//   - "oauth": show OAuth-capable providers (from getOAuthProviders)
//   - "api_key": show API key providers (hardcoded list, minus OAuth providers)
// ============================================================================

import { getOAuthProviders } from "piko-host-runtime";
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

const API_KEY_PROVIDERS = [
  { value: "openai", label: "OpenAI", description: "Standard OpenAI API Key" },
  { value: "anthropic", label: "Anthropic", description: "Standard Anthropic API Key" },
  { value: "google", label: "Google / Gemini", description: "Google Gemini API Key" },
  { value: "deepseek", label: "DeepSeek", description: "DeepSeek API Key" },
  { value: "groq", label: "Groq", description: "Groq API Key" },
  { value: "openrouter", label: "OpenRouter", description: "OpenRouter API Key" },
  { value: "cohere", label: "Cohere", description: "Cohere API Key" },
  { value: "together", label: "Together AI", description: "Together AI API Key" },
  { value: "mistral", label: "Mistral", description: "Mistral API Key" },
];

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
    return new Set(getOAuthProviders().map((p) => p.id));
  });

  const items = createMemo<SelectItem<string>[]>(() => {
    if (mode === "oauth") {
      const oauthProviders = getOAuthProviders();
      return oauthProviders.map((p) => {
        const authStorage = actionSvc.modelRegistry?.getAuthStorage();
        const hasKey = authStorage?.has(p.id);
        return {
          id: p.id,
          label: p.name,
          description: "OAuth / Subscription",
          value: p.id,
          badge: hasKey ? "logged in" : undefined,
        };
      });
    }

    // API key mode — filter out providers that have OAuth support
    const excluded = oauthProviderIds();
    const filtered = API_KEY_PROVIDERS.filter((p) => !excluded.has(p.value));
    return filtered.map((p) => {
      const authStorage = actionSvc.modelRegistry?.getAuthStorage();
      const hasKey = authStorage?.has(p.value);
      return {
        id: p.value,
        label: p.label,
        description: p.description,
        value: p.value,
        badge: hasKey ? "logged in" : undefined,
      };
    });
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
