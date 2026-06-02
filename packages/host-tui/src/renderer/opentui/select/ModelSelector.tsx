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

function clamp(n: number, max: number): number {
  return Math.max(0, Math.min(max, n));
}

export function ModelSelector(props: ModelSelectorProps) {
  const { actionSvc, controller, surfaceId, onClose } = props;

  const [query, setQuery] = createSignal("");
  const [selectedIdx, setSelectedIdx] = createSignal(0);

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

  const items = createMemo<SelectItem<ModelEntry>[]>(() => {
    const q = query().toLowerCase().trim();
    const filtered = q
      ? allModels.filter(
          (e) =>
            e.id.toLowerCase().includes(q) ||
            e.provider.toLowerCase().includes(q) ||
            e.name.toLowerCase().includes(q),
        )
      : allModels;

    return filtered.map((entry) => {
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
    });
  });

  const itemCount = () => items().length;

  function confirm(): void {
    const idx = clamp(selectedIdx(), itemCount() - 1);
    const item = items()[idx];
    if (item) {
      actionSvc.switchModel(item.value.id, item.value.provider);
    }
    onClose();
  }

  // Register keyboard handler as surface controller
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
        if (event.name === "pageup") {
          setSelectedIdx((i) => clamp(i - 5, itemCount() - 1));
          return true;
        }
        if (event.name === "pagedown") {
          setSelectedIdx((i) => clamp(i + 5, itemCount() - 1));
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
        if (event.name === "backspace") {
          setQuery((q) => q.slice(0, -1));
          setSelectedIdx(0);
          return true;
        }
        // Printable characters → filter query
        if (event.char && event.char.length === 1 && event.char >= " ") {
          setQuery((q) => q + event.char);
          setSelectedIdx(0);
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
        <text>{query() || "Type to filter models..."}</text>
      </box>

      <SelectListView
        items={items()}
        selectedIndex={selectedIdx()}
        maxHeight={10}
        onSelect={() => {}}
      />
    </SelectorShell>
  );
}
