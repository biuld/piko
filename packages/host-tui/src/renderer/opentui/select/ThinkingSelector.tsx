// ============================================================================
// Thinking Level Selector — uses SelectorShell
// ============================================================================

import { createMemo } from "solid-js";
import type { ActionService } from "../action-service.js";
import { SelectorShell } from "./SelectorShell.js";

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
  onClose: () => void;
}

export function ThinkingSelector(props: ThinkingSelectorProps) {
  const { actionSvc, onClose } = props;
  const currentLevel = () => actionSvc.getState().model.thinkingLevel;

  const options = createMemo(() =>
    LEVELS.map((level) => {
      const isCurrent = level.value === currentLevel();
      return {
        name: `${isCurrent ? "✓ " : "  "}${level.label}`,
        description: level.description,
        value: level.value as any,
      };
    }),
  );

  function handleSelect(_index: number, option: { value?: string } | null): void {
    if (option?.value) {
      actionSvc.setThinkingLevel(option.value);
    }
    onClose();
  }

  return (
    <SelectorShell
      title="Thinking Level"
      onClose={onClose}
      hints={["↑↓ navigate  Enter select  Esc cancel"]}
    >
      <select
        options={options()}
        selectedIndex={0}
        showDescription
        height={LEVELS.length + 2}
        onSelect={handleSelect}
      />
    </SelectorShell>
  );
}
