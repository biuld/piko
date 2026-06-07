// ============================================================================
// Settings Selector — browseable, editable settings grouped by category.
//
// All settings stored in SettingsManager are exposed. Text/number settings
// enter inline edit mode on Enter; bool/enum toggle directly.
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount } from "solid-js";
import type { PikoHost, SettingsManager } from "piko-host-runtime";
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
  group: string;
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
  host?: PikoHost;
  controller: TuiController;
  surfaceId: string;
  maxHeight?: number;
  onClose: () => void;
}

// ============================================================================
// Component
// ============================================================================

export function SettingsSelector(props: SettingsSelectorProps) {
  const { store, settingsManager: sm, host, controller, surfaceId } = props;
  const [selectedIdx, setSelectedIdx] = createSignal(0);

  // Build setting definitions from SettingsManager
  const defs = createMemo<SettingDef[]>(() => {
    if (!sm) return [];

    const s = sm.settings;
    const result: SettingDef[] = [];

    // ===== Model =====
    const G = "Model";
    result.push({
      key: "defaultProvider",
      label: "Default Provider",
      kind: "string",
      group: G,
      get: () => s.defaultProvider,
      set: (v) => sm.setDefaultProvider(String(v)),
    });
    result.push({
      key: "defaultModel",
      label: "Default Model",
      kind: "string",
      group: G,
      get: () => s.defaultModel,
      set: (v) => sm.setDefaultModel(String(v)),
    });
    result.push({
      key: "thinkingLevel",
      label: "Thinking Level",
      kind: "enum",
      group: G,
      options: ["off", "minimal", "low", "medium", "high", "xhigh"],
      get: () => s.defaultThinkingLevel ?? "off",
      set: (v) => sm.setDefaultThinkingLevel(v as any),
    });

    // ===== Display =====
    const D = "Display";
    result.push({
      key: "theme",
      label: "Theme",
      kind: "string",
      group: D,
      get: () => sm.getTheme() ?? "dark",
      set: (v) => sm.setTheme(String(v)),
    });
    result.push({
      key: "hideThinkingBlock",
      label: "Hide Thinking Block",
      kind: "boolean",
      group: D,
      get: () => sm.getHideThinkingBlock(),
      set: (v) => sm.setHideThinkingBlock(Boolean(v)),
    });
    result.push({
      key: "quietStartup",
      label: "Quiet Startup",
      kind: "boolean",
      group: D,
      get: () => sm.getQuietStartup(),
      set: (v) => sm.setQuietStartup(Boolean(v)),
    });
    result.push({
      key: "clearOnShrink",
      label: "Clear On Terminal Shrink",
      kind: "boolean",
      group: D,
      get: () => sm.getClearOnShrink(),
      set: (v) => sm.setClearOnShrink(Boolean(v)),
    });

    // ===== Connection =====
    const C = "Connection";
    result.push({
      key: "transport",
      label: "Transport",
      kind: "enum",
      group: C,
      options: ["auto", "stdio", "sse"],
      get: () => sm.getTransport(),
      set: (v) => sm.setTransport(v as any),
    });

    // ===== Queue =====
    const Q = "Queue";
    result.push({
      key: "steeringMode",
      label: "Steering Mode",
      kind: "enum",
      group: Q,
      options: ["all", "one-at-a-time"],
      get: () => sm.getSteeringMode(),
      set: (v) => {
        sm.setSteeringMode(v);
        host?.setSteeringMode(v);
      },
    });
    result.push({
      key: "followUpMode",
      label: "Follow-up Mode",
      kind: "enum",
      group: Q,
      options: ["all", "one-at-a-time"],
      get: () => sm.getFollowUpMode(),
      set: (v) => {
        sm.setFollowUpMode(v);
        host?.setFollowUpMode(v);
      },
    });

    // ===== Compaction =====
    const CM = "Compaction";
    result.push({
      key: "compactionEnabled",
      label: "Auto Compaction",
      kind: "boolean",
      group: CM,
      get: () => sm.getCompactionSettings().enabled,
      set: (v) => sm.setCompactionEnabled(Boolean(v)),
    });
    result.push({
      key: "compactionReserve",
      label: "Reserve Tokens",
      kind: "number",
      group: CM,
      get: () => sm.getCompactionSettings().reserveTokens,
      set: (v) => sm.setCompactionReserveTokens(Number(v)),
    });
    result.push({
      key: "compactionKeepRecent",
      label: "Keep Recent Tokens",
      kind: "number",
      group: CM,
      get: () => sm.getCompactionSettings().keepRecentTokens,
      set: (v) => sm.setCompactionKeepRecentTokens(Number(v)),
    });

    // ===== Retry =====
    const R = "Retry";
    result.push({
      key: "retryEnabled",
      label: "Retry on Failure",
      kind: "boolean",
      group: R,
      get: () => sm.getRetrySettings().enabled,
      set: (v) => sm.setRetryEnabled(Boolean(v)),
    });
    result.push({
      key: "retryMaxRetries",
      label: "Max Retries",
      kind: "number",
      group: R,
      get: () => sm.getRetrySettings().maxRetries,
      set: (v) => sm.setRetryMaxRetries(Number(v)),
    });

    // ===== Interaction =====
    const I = "Interaction";
    result.push({
      key: "doubleEscapeAction",
      label: "Double-Escape Action",
      kind: "enum",
      group: I,
      options: ["tree", "fork", "none"],
      get: () => sm.getDoubleEscapeAction(),
      set: (v) => sm.setDoubleEscapeAction(v),
    });

    // ===== Paths =====
    const P = "Paths";
    result.push({
      key: "sessionDir",
      label: "Session Directory",
      kind: "string",
      group: P,
      get: () => s.sessionDir,
      set: (v) => sm.setSessionDir(String(v)),
    });
    result.push({
      key: "shellPath",
      label: "Shell Path",
      kind: "string",
      group: P,
      get: () => sm.getShellPath(),
      set: (v) => sm.setShellPath(String(v)),
    });

    return result;
  });

  // Edit mode for text/number settings
  const [editMode, setEditMode] = createSignal<string | null>(null);
  const [editText, setEditText] = createSignal("");

  // Derive flat items with group headers
  const items = createMemo<SelectItem<SettingDef>[]>(() => {
    const flat: SelectItem<SettingDef>[] = [];
    let prevGroup = "";

    for (const def of defs()) {
      if (def.group !== prevGroup) {
        flat.push({
          id: `group-${def.group}`,
          label: def.group,
          description: "",
          value: undefined as any,
        });
        prevGroup = def.group;
      }

      const val = def.get();
      const isEditing = editMode() === def.key;
      flat.push({
        id: def.key,
        label: `  ${def.label}`,
        description: isEditing ? `${editText()}_` : formatSettingValue(def, val),
        value: def,
      });
    }

    return flat;
  });

  const itemCount = () => items().length;

  const isGroupHeader = (idx: number): boolean => {
    const item = items()[idx];
    return item !== undefined && item.value === undefined;
  };

  const findNextNonGroup = (fromIdx: number, direction: 1 | -1): number => {
    let idx = fromIdx;
    while (idx >= 0 && idx < itemCount()) {
      if (!isGroupHeader(idx)) return idx;
      idx += direction;
    }
    return fromIdx;
  };

  // Handle selection (Enter)
  const handleSelect = () => {
    const idx = selectedIdx();
    if (isGroupHeader(idx)) return;

    const sel = items()[idx];
    if (!sel?.value) return;
    const def = sel.value;

    if (def.kind === "boolean") {
      const current = Boolean(def.get());
      def.set(!current);
    } else if (def.kind === "enum" && def.options) {
      const current = String(def.get());
      const optIdx = def.options.indexOf(current);
      const next = def.options[(optIdx + 1) % def.options.length];
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
    if (def.kind === "number") {
      const val = Number(editText());
      if (!isNaN(val)) {
        def.set(val);
      }
    } else {
      def.set(editText());
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
        let nextIdx = nextState.selectedIndex;

        // Skip group headers
        if (isGroupHeader(nextIdx)) {
          // Determine direction: if we moved down, skip forward; if up, skip backward
          const delta = nextIdx - selectedIdx();
          const direction: 1 | -1 = delta >= 0 ? 1 : -1;
          nextIdx = findNextNonGroup(nextIdx, direction);
        }

        setSelectedIdx(nextIdx);
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
    return 22; // enough for all 19 settings + 8 group headers
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
  if (def.kind === "boolean") return value ? "on" : "off";
  if (def.kind === "enum") return String(value);
  if (def.kind === "number") return String(value);
  return String(value);
}
