// ============================================================================
// Model Selector — uses SelectListView with keyboard through focus
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { listAvailableModels } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectorShell } from "./SelectorShell.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import {
  createSelectableListState,
  filterSelectableItems,
  getSelectedItem,
  handleSelectableListKey,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";

export interface ModelSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  onClose: () => void;
}

interface ModelEntry {
  id: string;
  provider: string;
  name: string;
}

export function ModelSelector(props: ModelSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose } = props;

  const [listState, setListState] = createSignal<SelectableListState>(
    createSelectableListState(),
  );

  const allModels = (() => {
    const available = listAvailableModels();
    const entries: ModelEntry[] = [];
    for (const p of available) {
      for (const m of p.models) {
        entries.push({ id: m.id, provider: p.provider, name: m.name });
      }
    }
    return entries;
  })();

  const currentModel = () => actionSvc.getState().model.current;

  const allItems = createMemo<SelectItem<ModelEntry>[]>(() =>
    allModels.map((entry) => {
      const isCurrent =
        entry.id === currentModel().id &&
        entry.provider === currentModel().provider;
      return {
        id: `${entry.provider}/${entry.id}`,
        label: entry.id,
        description: `[${entry.provider}] ${entry.name}`,
        value: entry,
        badge: isCurrent ? "current" : undefined,
      };
    }),
  );

  const items = createMemo<SelectItem<ModelEntry>[]>(() =>
    filterSelectableItems(allItems(), listState().query),
  );

  function confirm(): void {
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (item) {
      const didSwitch = actionSvc.switchModel(item.value.id, item.value.provider);
      if (!didSwitch) {
        controller.notifications.notify({
          message: `Unable to switch to ${item.value.provider}/${item.value.id}`,
          severity: "error",
          source: "model",
        });
        return;
      }
    }
    onClose();
  }

  // Register keyboard handler as surface controller
  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): boolean {
        const next = handleSelectableListKey(listState(), event, {
          total: items().length,
          filterable: true,
        });
        if (next) {
          setListState(next);
          return true;
        }
        if (event.name === "enter" || event.name === "return") {
          confirm();
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
      title="Select Model"
      onClose={onClose}
      hints={["↑↓ navigate  Enter select  Esc cancel  Type to filter"]}
    >
      {/* Filter row — query rendered as plain text */}
      <box height={1} paddingBottom={1}>
        <text>{listState().query || "Type to filter models..."}</text>
      </box>

      <SelectListView
        items={items()}
        selectedIndex={listState().selectedIndex}
        maxHeight={10}
        onSelect={() => {}}
      />
    </SelectorShell>
  );
}
