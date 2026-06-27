// ============================================================================
// Model Selector — FilterBar + ListBody + HintBar.
//
// Models come from hostd via model_list command → model_list_received event.
// No local catalog — fully thin client.
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

  const allItems = createMemo<SelectItem<{ id: string; provider: string }>[]>(() => {
    const state = actionSvc.getState();
    const catalog = state.model.modelCatalog;
    if (!catalog || catalog.length === 0) return [];

    const items: SelectItem<{ id: string; provider: string }>[] = [];
    for (const providerInfo of catalog) {
      for (const model of providerInfo.models) {
        items.push({
          id: `${providerInfo.provider}/${model.id}`,
          label: model.id,
          description: providerInfo.provider,
          value: { id: model.id, provider: providerInfo.provider },
        });
      }
    }
    return items;
  });

  const items = createMemo<SelectItem<{ id: string; provider: string }>[]>(() =>
    filterSelectableItems(allItems(), listState().query),
  );

  function confirm(): void {
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (item) {
      actionSvc.switchModel(item.value.id, item.value.provider);
    }
    onClose();
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
