// ============================================================================
// Model Selector — uses SelectorShell, filters models, calls ActionService
// ============================================================================

import { createSignal, createMemo } from "solid-js";
import { listAvailableModels } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import { SelectorShell } from "./SelectorShell.js";

export interface ModelSelectorProps {
  actionSvc: ActionService;
  onClose: () => void;
}

interface ModelEntry {
  id: string;
  provider: string;
  name: string;
}

export function ModelSelector(props: ModelSelectorProps) {
  const { actionSvc, onClose } = props;
  const [search, setSearch] = createSignal("");
  const [selIdx, setSelIdx] = createSignal(0);

  const allModels = createMemo<ModelEntry[]>(() => {
    const available = listAvailableModels();
    const entries: ModelEntry[] = [];
    for (const p of available) {
      for (const m of p.models) {
        entries.push({ id: m.id, provider: p.provider, name: m.name });
      }
    }
    return entries;
  });

  const filtered = createMemo(() => {
    const q = search().trim().toLowerCase();
    if (!q) return allModels();
    return allModels().filter((entry) => {
      const id = entry.id.toLowerCase();
      const name = entry.name.toLowerCase();
      const provider = entry.provider.toLowerCase();
      return id.includes(q) || name.includes(q) || provider.includes(q);
    });
  });

  const currentModel = () => actionSvc.getState().model.current;

  const options = createMemo(() =>
    filtered().map((entry) => {
      const isCurrent =
        entry.id === currentModel().id &&
        entry.provider === currentModel().provider;
      return {
        name: `${isCurrent ? "✓ " : "  "}${entry.id}`,
        description: `[${entry.provider}] ${entry.name}`,
        value: entry,
      };
    }),
  );

  function handleSelect(_index: number, option: { value?: ModelEntry } | null): void {
    if (option?.value) {
      actionSvc.switchModel(option.value.id, option.value.provider);
    }
    onClose();
  }

  return (
    <SelectorShell
      title="Select Model"
      onClose={onClose}
      hints={["↑↓ navigate  Enter select  Esc cancel"]}
    >
      <box height={1} paddingBottom={1}>
        <text>Filter: </text>
        <input
          value={search()}
          placeholder="Type to filter models..."
          onInput={(value: string) => setSearch(value)}
        />
      </box>

      <box flexGrow={1}>
        {filtered().length > 0 ? (
          <select
            options={options()}
            selectedIndex={selIdx()}
            showDescription
            height={Math.min(filtered().length + 2, 12)}
            onSelect={handleSelect}
          />
        ) : (
          <text>No models found</text>
        )}
      </box>
    </SelectorShell>
  );
}
