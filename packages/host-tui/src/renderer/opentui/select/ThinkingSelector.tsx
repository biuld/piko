// ============================================================================
// Thinking Level Selector — uses SelectListView + keyboard through focus
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { ActionService } from "../action-service.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectorShell } from "./SelectorShell.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";

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

function clamp(n: number, max: number): number {
  return Math.max(0, Math.min(max, n));
}

export function ThinkingSelector(props: ThinkingSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose } = props;

  const [selectedIdx, setSelectedIdx] = createSignal(0);

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

  const itemCount = () => items().length;

  function confirm(): void {
    const idx = clamp(selectedIdx(), itemCount() - 1);
    const item = items()[idx];
    if (item) {
      actionSvc.setThinkingLevel(item.value);
    }
    onClose();
  }

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): boolean {
        if (event.name === "up") {
          setSelectedIdx((i) => clamp(i - 1, itemCount() - 1));
          return true;
        }
        if (event.name === "down") {
          setSelectedIdx((i) => clamp(i + 1, itemCount() - 1));
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
      title="Thinking Level"
      onClose={onClose}
      hints={["↑↓ navigate  Enter select  Esc cancel"]}
    >
      <SelectListView
        items={items()}
        selectedIndex={selectedIdx()}
        maxHeight={LEVELS.length + 2}
        onSelect={() => {}}
      />
    </SelectorShell>
  );
}
