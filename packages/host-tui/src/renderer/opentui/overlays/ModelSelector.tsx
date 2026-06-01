// ============================================================================
// Model Selector Overlay
// Uses ModelRegistry.resolve() for proper provider config resolution
// ============================================================================

import { createSignal, createMemo } from "solid-js";
import type { Model } from "@earendil-works/pi-ai";
import { listAvailableModels } from "piko-host-runtime";
import type { ActionService } from "../action-service.js";
import { OverlayContainer } from "./OverlayContainer.js";

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

  // Build model list from available models (cached on first access)
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

  // Filter models by search
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

  // Build select options
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
      const entry = option.value;
      // Use ModelRegistry.resolve() for proper provider config + host.setConfig()
      actionSvc.switchModel(entry.id, entry.provider);
    }
    onClose();
  }

  const items = filtered();

  return (
    <OverlayContainer kind="model" title="Select Model" onClose={onClose}>
      <box height={1} paddingBottom={1}>
        <text fg="#808080">Search: </text>
        <input
          value={search()}
          placeholder="Type to filter models..."
          onChange={(value: string) => setSearch(value)}
        />
      </box>

      <box flexGrow={1}>
        {items.length > 0 ? (
          <select
            options={options()}
            selectedIndex={0}
            showDescription
            height={Math.min(items.length + 2, 12)}
            onSelect={handleSelect}
          />
        ) : (
          <text fg="#808080">No models found</text>
        )}
      </box>
    </OverlayContainer>
  );
}
