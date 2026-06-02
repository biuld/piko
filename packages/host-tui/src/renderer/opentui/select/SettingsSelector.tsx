// ============================================================================
// Settings Selector — key-value settings browser using SelectorShell
// ============================================================================

import { createMemo } from "solid-js";
import type { SettingsManager } from "piko-host-runtime";
import type { TuiStore } from "../store.js";
import { SelectorShell } from "./SelectorShell.js";

export interface SettingsSelectorProps {
  store: TuiStore;
  settingsManager?: SettingsManager;
  onClose: () => void;
}

interface SettingRow {
  key: string;
  value: string;
  description: string;
}

export function SettingsSelector(props: SettingsSelectorProps) {
  const { store, settingsManager, onClose } = props;

  const settings = createMemo<SettingRow[]>(() => {
    if (!settingsManager) return [];

    const rows: SettingRow[] = [];

    try {
      const defaultModel = settingsManager.getDefaultModel?.();
      const defaultProvider = settingsManager.getDefaultProvider?.();
      const thinking = settingsManager.getDefaultThinkingLevel?.();
      const theme = settingsManager.getTheme?.();

      if (defaultModel) {
        rows.push({ key: "Default Model", value: defaultModel, description: "Default model ID" });
      }
      if (defaultProvider) {
        rows.push({
          key: "Default Provider",
          value: defaultProvider,
          description: "Default provider name",
        });
      }
      if (thinking) {
        rows.push({
          key: "Thinking Level",
          value: thinking,
          description: "Default thinking level",
        });
      }
      if (theme) {
        rows.push({ key: "Theme", value: theme, description: "Color theme" });
      }
    } catch {
      // Settings may not be fully initialized
    }

    return rows;
  });

  const options = createMemo(() =>
    settings().map((row) => ({
      name: row.key,
      description: `${row.value} — ${row.description}`,
    })),
  );

  function handleSelect(_index: number, _option: unknown): void {
    onClose();
  }

  return (
    <SelectorShell
      title="Settings"
      onClose={onClose}
      hints={["↑↓ navigate  Enter select  Esc cancel"]}
    >
      {settings().length > 0 ? (
        <select
          options={options()}
          selectedIndex={0}
          showDescription
          height={Math.min(settings().length + 2, 12)}
          onSelect={handleSelect}
        />
      ) : (
        <text>No settings available</text>
      )}
    </SelectorShell>
  );
}
