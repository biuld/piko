// ============================================================================
// Thinking Level Selector — ListBody + HintBar.
//
// Self-contained: owns all state, keyboard handling, and UI composition.
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import { selectorBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";
import {
  createSelectableListState,
  getSelectedItem,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import { ListBody, HintBar } from "../primitives/index.js";

const LEVELS = [
  { value: "off", label: "off", description: "No reasoning" },
  { value: "minimal", label: "minimal", description: "Minimal reasoning (~1k tokens)" },
  { value: "low", label: "low", description: "Light reasoning (~2k tokens)" },
  { value: "medium", label: "medium", description: "Moderate reasoning (~8k tokens)" },
  { value: "high", label: "high", description: "Deep reasoning (~16k tokens)" },
  { value: "xhigh", label: "xhigh", description: "Maximum reasoning (~32k tokens)" },
];

export interface ThinkingSelectorProps {
  actionSvc: ActionService;
  controller: TuiController;
  surfaceId: string;
  availableWidth: number;
  availableHeight: number;
  onClose: () => void;
}

export function ThinkingSelector(props: ThinkingSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose, availableWidth, availableHeight } = props;

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

  // Layout: list + gap (1) + HintBar (1)
  const listMaxH = () => Math.max(1, availableHeight - 2);

  return (
    <box flexDirection="column">
      <ListBody
        items={items()}
        selectedIndex={listState().selectedIndex}
        maxHeight={listMaxH()}
        width={availableWidth}
        showDescriptions={true}
      />
      <HintBar hints="Up/Down move  Enter select  Esc close" />
    </box>
  );
}
