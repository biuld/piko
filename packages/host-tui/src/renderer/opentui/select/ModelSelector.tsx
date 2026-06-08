// ============================================================================
// Model Selector — FilterBar + ListBody + HintBar.
//
// Self-contained: owns all state, keyboard handling, and UI composition.
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { KeyEvent } from "../../../focus/types.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import { type SurfaceKeyResult, selectorBehavior } from "../../../surfaces/index.js";
import {
  createSelectableListState,
  filterSelectableItems,
  getSelectedItem,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import type { ActionService } from "../action-service.js";
import { FilterBar, ListBody } from "../primitives/index.js";
import type { SelectItem } from "./selector-controller.js";

export interface ModelSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  initialQuery?: string;
  availableWidth: number;
  availableHeight: number;
  onClose: () => void;
}

export function ModelSelector(props: ModelSelectorProps) {
  const {
    actionSvc,
    controller,
    surfaceId,
    onClose,
    initialQuery,
    availableWidth,
    availableHeight,
  } = props;

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

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): SurfaceKeyResult {
        const { nextState, result } = selectorBehavior(event, listState(), items().length);
        if (nextState.query !== listState().query) {
          // Query change is handled internally via setListState
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

  // Layout: FilterBar (1) + gap (1) + list
  const listMaxH = () => Math.max(1, availableHeight - 2);

  return (
    <box flexDirection="column">
      <FilterBar query={listState().query} placeholder="Search models..." />
      <box height={1} />
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
