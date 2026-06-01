// ============================================================================
// Model Selector Overlay
// Uses OpenTUI <select> component for keyboard-navigable list
// ============================================================================

import { createSignal, createMemo, onMount } from "solid-js";
import type { Model } from "@earendil-works/pi-ai";
import { listAvailableModels } from "piko-host-runtime";
import type { TuiStore } from "../store.js";
import { OverlayContainer } from "./OverlayContainer.js";

export interface ModelSelectorProps {
  store: TuiStore;
  onClose: () => void;
}

interface ModelEntry {
  model: Model<string>;
}

export function ModelSelector(props: ModelSelectorProps) {
  const { store, onClose } = props;
  const [search, setSearch] = createSignal("");
  const [models, setModels] = createSignal<ModelEntry[]>([]);

  // Load models on mount
  onMount(() => {
    const available = listAvailableModels();
    const entries: ModelEntry[] = [];
    for (const p of available) {
      for (const m of p.models) {
        entries.push({
          model: { id: m.id, provider: p.provider, name: m.name } as Model<string>,
        });
      }
    }
    setModels(entries);
  });

  // Filter models by search
  const filtered = createMemo(() => {
    const q = search().trim().toLowerCase();
    if (!q) return models();
    return models().filter((entry) => {
      const id = entry.model.id.toLowerCase();
      const name = entry.model.name.toLowerCase();
      const provider = entry.model.provider.toLowerCase();
      return id.includes(q) || name.includes(q) || provider.includes(q);
    });
  });

  const currentModel = () => store.state().model.current;

  // Build select options from filtered models
  const options = createMemo(() =>
    filtered().map((entry) => {
      const isCurrent =
        entry.model.id === currentModel().id &&
        entry.model.provider === currentModel().provider;
      return {
        name: `${isCurrent ? "✓ " : "  "}${entry.model.id}`,
        description: `[${entry.model.provider}] ${entry.model.name}`,
        value: entry as any,
      };
    }),
  );

  function handleSelect(_index: number, option: { value?: any } | null): void {
    if (option?.value) {
      const entry = option.value as ModelEntry;
      store.dispatch({
        type: "model_changed",
        model: entry.model,
        providerConfig: store.state().model.providerConfig,
      });
    }
    onClose();
  }

  const items = filtered();

  return (
    <OverlayContainer kind="model" title="Select Model" onClose={onClose}>
      {/* Search input */}
      <box height={1} paddingBottom={1}>
        <text fg="#808080">Search: </text>
        <input
          value={search()}
          placeholder="Type to filter models..."
          onChange={(value: string) => {
            setSearch(value);
          }}
        />
      </box>

      {/* Model list via <select> */}
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
