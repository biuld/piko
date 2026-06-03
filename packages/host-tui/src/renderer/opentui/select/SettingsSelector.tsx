// ============================================================================
// Settings Selector — key-value settings viewer using SelectListView
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { SettingsManager } from "piko-host-runtime";
import type { TuiStore } from "../store.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectorShell } from "./SelectorShell.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import {
  createSelectableListState,
  handleSelectableListKey,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";

export interface SettingsSelectorProps {
  store: TuiStore;
  settingsManager?: SettingsManager;
  controller: TuiController;
  surfaceId: string;
  onClose: () => void;
}

export function SettingsSelector(props: SettingsSelectorProps) {
  const { store, settingsManager, controller, surfaceId, onClose } = props;
  const [listState, setListState] = createSignal<SelectableListState>(
    createSelectableListState(),
  );

  const items = createMemo<SelectItem<null>[]>(() => {
    if (!settingsManager) return [];

    const rows: Array<{ key: string; value: string; description: string }> = [];
    try {
      const defaultModel = settingsManager.getDefaultModel?.();
      if (defaultModel) rows.push({ key: "Default Model", value: defaultModel, description: "Default model ID" });
      const defaultProvider = settingsManager.getDefaultProvider?.();
      if (defaultProvider) rows.push({ key: "Default Provider", value: defaultProvider, description: "Default provider" });
      const thinking = settingsManager.getDefaultThinkingLevel?.();
      if (thinking) rows.push({ key: "Thinking Level", value: thinking, description: "Default thinking level" });
      const theme = settingsManager.getTheme?.();
      if (theme) rows.push({ key: "Theme", value: theme, description: "Color theme" });
    } catch {
      // Settings may not be fully initialized
    }

    return rows.map((row) => ({
      id: row.key,
      label: row.key,
      description: `${row.value} — ${row.description}`,
      value: null,
    }));
  });

  const itemCount = () => items().length;

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): boolean {
        const next = handleSelectableListKey(listState(), event, {
          total: itemCount(),
        });
        if (next) {
          setListState(next);
          return true;
        }
        if (event.name === "escape") {
          onClose();
          return true;
        }
        return false;
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  return (
    <SelectorShell
      title="Settings"
      onClose={onClose}
      hints={["↑↓ navigate  Esc close"]}
    >
      {items().length > 0 ? (
        <SelectListView
          items={items()}
          selectedIndex={listState().selectedIndex}
          maxHeight={10}
          onSelect={() => {}}
        />
      ) : (
        <text>No settings available</text>
      )}
    </SelectorShell>
  );
}
