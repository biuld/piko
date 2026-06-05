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
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import { selectorBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";

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

  const [models, setModels] = createSignal<any[]>([]);

  // Load models
  onMount(async () => {
    const state = controller.store.state();
    setModels(state.model.availableModels);
  });

  const allItems = createMemo<SelectItem<any>[]>(() =>
    models().map((m) => ({
      id: `${m.provider}/${m.id}`,
      label: m.id,
      description: m.provider,
      value: m,
    })),
  );

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
    <SelectorShell
      title="Select Model"
      onClose={onClose}
      hints={[
        controller.keymap.formatHintLine([
          ["tui.select.up", "navigate"],
          ["tui.select.down", ""],
          ["tui.select.confirm", "select"],
          ["tui.select.cancel", "cancel"],
        ]) + "  Type to filter",
      ]}
    >
      {/* Filter row — query rendered as plain text */}
      <box height={1} paddingBottom={1}>
        <text>{listState().query || "Type to filter models..."}</text>
      </box>

      <SelectListView
        items={items()}
        selectedIndex={listState().selectedIndex}
        width={actionSvc.getState().layout.viewport.width}
        maxHeight={10}
        onSelect={() => {}}
      />
    </SelectorShell>
  );
}
