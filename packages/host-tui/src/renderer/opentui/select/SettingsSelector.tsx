// ============================================================================
// Settings Selector — browseable, editable settings grouped by category.
// Bool/enum values toggle with Enter. Text values show read-only for now.
// ============================================================================

import { createMemo, createSignal, For, onCleanup, onMount } from "solid-js";
import type { SettingsManager } from "piko-host-runtime";
import type { TuiStore } from "../store.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectorShell } from "./SelectorShell.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";

// ============================================================================
// Types
// ============================================================================

type SettingKind = "string" | "boolean" | "number" | "enum";

interface SettingDef {
  key: string;
  label: string;
  kind: SettingKind;
  /** Options for enum kind */
  options?: string[];
  /** Category group */
  group: string;
  get: () => string | number | boolean | undefined;
  set: (value: any) => void;
}

// ============================================================================
// Props
// ============================================================================

export interface SettingsSelectorProps {
  store: TuiStore;
  settingsManager?: SettingsManager;
  controller: TuiController;
  surfaceId: string;
  onClose: () => void;
}

// ============================================================================
// Component
// ============================================================================

function clamp(n: number, max: number): number {
  return Math.max(0, Math.min(max, n));
}

export function SettingsSelector(props: SettingsSelectorProps) {
  const { store, settingsManager: sm, controller, surfaceId, onClose } = props;
  const [selectedIdx, setSelectedIdx] = createSignal(0);

  // Build setting definitions from SettingsManager
  const defs = createMemo<SettingDef[]>(() => {
    if (!sm) return [];

    const s = sm.settings;
    const result: SettingDef[] = [];

    // --- General ---
    result.push({
      key: "defaultProvider",
      label: "Default Provider",
      kind: "string",
      group: "General",
      get: () => s.defaultProvider,
      set: (v) => sm.setDefaultProvider(String(v)),
    });
    result.push({
      key: "defaultModel",
      label: "Default Model",
      kind: "string",
      group: "General",
      get: () => s.defaultModel,
      set: (v) => sm.setDefaultModel(String(v)),
    });
    result.push({
      key: "thinkingLevel",
      label: "Thinking Level",
      kind: "enum",
      options: ["off", "minimal", "low", "medium", "high", "xhigh"],
      group: "General",
      get: () => s.defaultThinkingLevel ?? "off",
      set: (v) => sm.setDefaultThinkingLevel(v as any),
    });
    result.push({
      key: "theme",
      label: "Theme",
      kind: "string",
      group: "General",
      get: () => s.theme ?? "dark",
      set: (v) => sm.setTheme(String(v)),
    });
    result.push({
      key: "transport",
      label: "Transport",
      kind: "enum",
      options: ["auto", "stdio", "sse"],
      group: "General",
      get: () => sm.getTransport(),
      set: (v) => sm.setTransport(v as any),
    });

    // --- Compaction ---
    result.push({
      key: "compactionEnabled",
      label: "Compaction",
      kind: "boolean",
      group: "Compaction",
      get: () => sm.getCompactionSettings().enabled,
      set: (v) => sm.setCompactionEnabled(Boolean(v)),
    });
    result.push({
      key: "compactionReserve",
      label: "Compaction Reserve (tokens)",
      kind: "number",
      group: "Compaction",
      get: () => sm.getCompactionSettings().reserveTokens,
      set: () => {},
    });
    result.push({
      key: "compactionKeepRecent",
      label: "Compaction Keep Recent (tokens)",
      kind: "number",
      group: "Compaction",
      get: () => sm.getCompactionSettings().keepRecentTokens,
      set: () => {},
    });

    // --- Retry ---
    result.push({
      key: "retryEnabled",
      label: "Retry on Failure",
      kind: "boolean",
      group: "Retry",
      get: () => sm.getRetrySettings().enabled,
      set: (v) => sm.setRetryEnabled(Boolean(v)),
    });
    result.push({
      key: "retryMaxRetries",
      label: "Max Retries",
      kind: "number",
      group: "Retry",
      get: () => sm.getRetrySettings().maxRetries,
      set: () => {},
    });

    // --- Display ---
    result.push({
      key: "hideThinkingBlock",
      label: "Hide Thinking Block",
      kind: "boolean",
      group: "Display",
      get: () => sm.getHideThinkingBlock(),
      set: (v) => sm.setHideThinkingBlock(Boolean(v)),
    });

    return result;
  });

  // Derive items with group headers
  const items = createMemo<SelectItem<SettingDef>[]>(() => {
    const d = defs();
    if (d.length === 0) return [];

    const result: SelectItem<SettingDef>[] = [];
    let lastGroup = "";
    for (const def of d) {
      if (def.group !== lastGroup) {
        lastGroup = def.group;
        result.push({
          id: `__group__${def.group}`,
          label: `── ${def.group} ──`,
          description: "",
          value: null as any,
        });
      }
      const val = def.get();
      const desc = formatSettingValue(def, val);
      result.push({
        id: def.key,
        label: `  ${def.label}`,
        description: desc,
        value: def,
      });
    }
    return result;
  });

  const itemCount = () => items().length;

  // Edit mode for text/number settings
  const [editMode, setEditMode] = createSignal<string | null>(null);
  const [editText, setEditText] = createSignal("");

  // Handle selection (Enter)
  const handleSelect = () => {
    const sel = items()[selectedIdx()];
    if (!sel) return;
    const def = sel.value;
    if (!def) return; // group header

    if (def.kind === "boolean") {
      const current = Boolean(def.get());
      def.set(!current);
    } else if (def.kind === "enum" && def.options) {
      const current = String(def.get());
      const idx = def.options.indexOf(current);
      const next = def.options[(idx + 1) % def.options.length];
      def.set(next);
    } else if (def.kind === "string" || def.kind === "number") {
      // Enter text edit mode
      setEditMode(def.key);
      setEditText(String(def.get() ?? ""));
    }
  };

  const commitEdit = () => {
    const key = editMode();
    if (!key) return;
    const def = defs().find((d) => d.key === key);
    if (!def) return;
    const val = def.kind === "number" ? Number(editText()) : editText();
    if (!isNaN(val as number)) {
      def.set(val);
    }
    setEditMode(null);
    setEditText("");
  };

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): boolean {
        // If in edit mode, handle text input
        if (editMode()) {
          if (event.char && event.char >= " ") {
            setEditText((t) => t + event.char!);
            return true;
          }
          if (event.name === "backspace") {
            setEditText((t) => t.slice(0, -1));
            return true;
          }
          if (event.name === "enter" || event.name === "return") {
            commitEdit();
            return true;
          }
          if (event.name === "escape") {
            setEditMode(null);
            return true;
          }
          return false;
        }

        // Navigation
        if (event.name === "up") {
          setSelectedIdx((i) => clamp(i - 1, itemCount() - 1));
          return true;
        }
        if (event.name === "down") {
          setSelectedIdx((i) => clamp(i + 1, itemCount() - 1));
          return true;
        }
        if (event.name === "enter" || event.name === "return") {
          handleSelect();
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
      title="Settings"
      onClose={onClose}
      hints={[
        controller.keymap.formatHintLine([
          ["tui.select.up", "navigate"],
          ["tui.select.down", ""],
          ["tui.select.confirm", editMode() ? "save" : "toggle/edit"],
          ["tui.select.cancel", editMode() ? "cancel edit" : "close"],
        ]),
      ]}
    >
      {items().length > 0 ? (
        <box flexDirection="column" maxHeight={16}>
          <For each={items()}>
            {(item, idx) => {
              const isSelected = idx() === selectedIdx();
              const isEditing = editMode() === item.id;
              const isGroupHeader = item.id.startsWith("__group__");

              if (isGroupHeader) {
                return (
                  <box height={1}>
                    <text fg="#666666">{item.label}</text>
                  </box>
                );
              }

              const prefix = isSelected ? (isEditing ? "✎ " : "> ") : "  ";
              const displayText = isEditing
                ? `${item.label}: ${editText()}_`
                : `${item.label}: ${item.description}`;

              return (
                <box height={1}>
                  <text
                    fg={
                      isSelected && !isEditing
                        ? "#88ccff"
                        : isSelected
                          ? "#ffcc66"
                          : undefined
                    }
                  >
                    {prefix}{displayText}
                  </text>
                </box>
              );
            }}
          </For>
        </box>
      ) : (
        <box padding={1}>
          <text>No settings available</text>
        </box>
      )}
    </SelectorShell>
  );
}

// ============================================================================
// Helpers
// ============================================================================

function formatSettingValue(def: SettingDef, value: unknown): string {
  if (value === undefined || value === null) return "(not set)";
  if (def.kind === "boolean") return value ? "✓ on" : "✗ off";
  if (def.kind === "enum") return String(value);
  return String(value);
}
