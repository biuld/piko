// ============================================================================
// Settings Selector — browseable, editable settings menu.
// Bool/enum values toggle with Enter. Text values show read-only for now.
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { SettingsManager } from "piko-host-runtime";
import type { TuiStore } from "../store.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import { menuBehavior, formBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";

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
  maxHeight?: number;
  onClose: () => void;
}

// ============================================================================
// Component
// ============================================================================

export function SettingsSelector(props: SettingsSelectorProps) {
  const { store, settingsManager: sm, controller, surfaceId } = props;
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
      get: () => s.defaultProvider,
      set: (v) => sm.setDefaultProvider(String(v)),
    });
    result.push({
      key: "defaultModel",
      label: "Default Model",
      kind: "string",
      get: () => s.defaultModel,
      set: (v) => sm.setDefaultModel(String(v)),
    });
    result.push({
      key: "thinkingLevel",
      label: "Thinking Level",
      kind: "enum",
      options: ["off", "minimal", "low", "medium", "high", "xhigh"],
      get: () => s.defaultThinkingLevel ?? "off",
      set: (v) => sm.setDefaultThinkingLevel(v as any),
    });
    result.push({
      key: "theme",
      label: "Theme",
      kind: "string",
      get: () => s.theme ?? "dark",
      set: (v) => sm.setTheme(String(v)),
    });
    result.push({
      key: "transport",
      label: "Transport",
      kind: "enum",
      options: ["auto", "stdio", "sse"],
      get: () => sm.getTransport(),
      set: (v) => sm.setTransport(v as any),
    });

    // --- Compaction ---
    result.push({
      key: "compactionEnabled",
      label: "Compaction",
      kind: "boolean",
      get: () => sm.getCompactionSettings().enabled,
      set: (v) => sm.setCompactionEnabled(Boolean(v)),
    });
    result.push({
      key: "compactionReserve",
      label: "Compaction Reserve (tokens)",
      kind: "number",
      get: () => sm.getCompactionSettings().reserveTokens,
      set: () => {},
    });
    result.push({
      key: "compactionKeepRecent",
      label: "Compaction Keep Recent (tokens)",
      kind: "number",
      get: () => sm.getCompactionSettings().keepRecentTokens,
      set: () => {},
    });

    // --- Retry ---
    result.push({
      key: "retryEnabled",
      label: "Retry on Failure",
      kind: "boolean",
      get: () => sm.getRetrySettings().enabled,
      set: (v) => sm.setRetryEnabled(Boolean(v)),
    });
    result.push({
      key: "retryMaxRetries",
      label: "Max Retries",
      kind: "number",
      get: () => sm.getRetrySettings().maxRetries,
      set: () => {},
    });

    // --- Display ---
    result.push({
      key: "hideThinkingBlock",
      label: "Hide Thinking Block",
      kind: "boolean",
      get: () => sm.getHideThinkingBlock(),
      set: (v) => sm.setHideThinkingBlock(Boolean(v)),
    });

    return result;
  });

  // Edit mode for text/number settings
  const [editMode, setEditMode] = createSignal<string | null>(null);
  const [editText, setEditText] = createSignal("");

  // Derive flat items. Group headers are intentionally omitted in the compact
  // editor panel; the row labels carry enough context.
  const items = createMemo<SelectItem<SettingDef>[]>(() => {
    return defs().map((def) => {
      const val = def.get();
      const isEditing = editMode() === def.key;
      return {
        id: def.key,
        label: def.label,
        description: isEditing ? `${editText()}_` : formatSettingValue(def, val),
        value: def,
      };
    });
  });

  const itemCount = () => items().length;

  // Handle selection (Enter)
  const handleSelect = () => {
    const sel = items()[selectedIdx()];
    if (!sel) return;
    const def = sel.value;

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
      handleKey(event: KeyEvent): SurfaceKeyResult {
        // If in edit mode, handle text input using formBehavior
        if (editMode()) {
          const formState = { value: editText() };
          const { nextState, result } = formBehavior(event, formState);
          setEditText(nextState.value);
          if (result.type === "submit") {
            commitEdit();
            return { type: "handled" };
          }
          if (result.type === "close") {
            setEditMode(null);
            return { type: "handled" };
          }
          return result;
        }

        // Navigation using menuBehavior
        const listState = { query: "", selectedIndex: selectedIdx() };
        const { nextState, result } = menuBehavior(event, listState, itemCount());
        setSelectedIdx(nextState.selectedIndex);
        return result;
      },
      onConfirm() {
        handleSelect();
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  const surface = () => controller.store.state().surfaces.find((s) => s.id === surfaceId);
  const placement = () => surface()?.placement ?? "partial";
  const viewportHeight = () => controller.store.state().layout.viewport.height;

  const maxHeight = () => {
    if (props.maxHeight !== undefined) return props.maxHeight;
    if (placement() === "full") {
      return Math.max(15, viewportHeight() - 6);
    }
    return 11; // 12 - 1 (hints)
  };

  return (
    <box flexDirection="column">
      {items().length > 0 ? (
        <SelectListView
          items={items()}
          selectedIndex={selectedIdx()}
          width={store.state().layout.viewport.width}
          maxHeight={maxHeight()}
          showDescriptions
          onSelect={() => {}}
        />
      ) : (
        <box padding={1}>
          <text>No settings available</text>
        </box>
      )}
    </box>
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
