// ============================================================================
// Model Selector — uses SelectListView with keyboard through focus
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { listAvailableModels } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import {
  createSelectableListState,
  filterSelectableItems,
  getSelectedItem,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import { selectorBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";

export interface ModelSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  initialQuery?: string;
  onQueryChange?: (query: string) => void;
  onClose: () => void;
}

interface ModelEntry {
  id: string;
  provider: string;
  name: string;
}

export function ModelSelector(props: ModelSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose, initialQuery } = props;

  const [listState, setListState] = createSignal<SelectableListState>({
    ...createSelectableListState(),
    query: initialQuery || "",
  });

  const allItems = createMemo<SelectItem<any>[]>(() => {
    const models = actionSvc.modelRegistry?.listScopedModels() || [];
    return models.map((m) => ({
      id: `${m.provider}/${m.id}`,
      label: m.id,
      description: m.provider,
      value: m,
    }));
  });

  const items = createMemo<SelectItem<any>[]>(() =>
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
      handleKey(event: KeyEvent): SurfaceKeyResult {
        const { nextState, result } = selectorBehavior(event, listState(), items().length);
        if (nextState.query !== listState().query) {
          props.onQueryChange?.(nextState.query);
        }
        setListState(nextState);
        return result;
      },
      onConfirm() {
        confirm();
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  return (
    <box flexDirection="column">
      <SelectListView
        items={items()}
        selectedIndex={listState().selectedIndex}
        width={actionSvc.getState().layout.viewport.width}
        onSelect={() => {}}
      />
    </box>
  );
}
