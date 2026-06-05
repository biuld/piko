// ============================================================================
// Thinking Level Selector — uses SelectListView + keyboard through focus
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import { selectorBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";
import {
  createSelectableListState,
  getSelectedItem,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";

const LEVELS = [
  { value: "off", label: "off", description: "No thinking" },
  { value: "minimal", label: "minimal", description: "Minimal reasoning" },
  { value: "low", label: "low", description: "Low reasoning" },
  { value: "medium", label: "medium", description: "Medium reasoning" },
  { value: "high", label: "high", description: "High reasoning" },
  { value: "xhigh", label: "xhigh", description: "Maximum reasoning" },
];

export interface ThinkingSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  onClose: () => void;
}

export function ThinkingSelector(props: ThinkingSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose } = props;

  const [listState, setListState] = createSignal<SelectableListState>(
    createSelectableListState(),
  );

  const currentLevel = () => actionSvc.getState().model.thinkingLevel;

  const items = createMemo<SelectItem<string>[]>(() =>
    LEVELS.map((level) => {
      const isCurrent = level.value === currentLevel();
      return {
        id: level.value,
        label: level.label,
        description: level.description,
        value: level.value,
        badge: isCurrent ? "current" : undefined,
      };
    }),
  );

  function confirm(): void {
    const item = getSelectedItem(items(), listState().selectedIndex);
    if (item) {
      actionSvc.setThinkingLevel(item.value);
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
