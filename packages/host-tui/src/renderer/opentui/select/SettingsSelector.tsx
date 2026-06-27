// ============================================================================
// Settings Selector — ListBody + DescriptionBox + HintBar.
//
// Self-contained: owns all state, keyboard handling, and UI composition.
// Bool/enum values cycle on Enter/Space. Submenus for thinking/theme.
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import type { TuiPreferences } from "../../../app/tui-preferences.js";
import type { KeyEvent } from "../../../focus/types.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import { type SurfaceKeyResult, selectorBehavior } from "../../../surfaces/index.js";
import {
  createSelectableListState,
  type SelectableListState,
} from "../../../surfaces/interactions/selectable-list.js";
import { DescriptionBox, HintBar, KeyValueList } from "../primitives/index.js";
import type { KeyValueItem } from "../primitives/KeyValueList.js";
import { useTheme } from "../theme-context.js";
import { SelectListView } from "./SelectListView.js";
import type { SelectItem } from "./selector-controller.js";

// ============================================================================
// Types
// ============================================================================

interface SettingDef {
  id: string;
  label: string;
  description: string;
  values?: string[];
  get: () => string;
  set: (value: string) => void;
  submenu?: boolean;
}

interface SubmenuOption {
  value: string;
  label: string;
  description: string;
}

export interface SettingsSelectorProps {
  preferences?: TuiPreferences;
  controller: TuiController;
  surfaceId: string;
  availableWidth: number;
  availableHeight: number;
  onClose: () => void;
}

// ============================================================================
// Helpers
// ============================================================================

function getSubmenuOptions(def: SettingDef): SubmenuOption[] {
  if (def.id === "theme") {
    return ["dark", "light"].map((t) => ({
      value: t,
      label: t,
      description: `${t} color theme`,
    }));
  }
  return [];
}

function formatBadge(def: SettingDef): string | undefined {
  if (def.submenu) return `${def.get()} \u203a`;
  const val = def.get();
  if (def.values) {
    if (def.values.length === 2 && def.values.includes("true") && def.values.includes("false")) {
      return val === "true" ? "on" : "off";
    }
  }
  return val;
}

function valueColorFor(def: SettingDef): string {
  if (def.submenu) return "text.dim";
  if (def.values && def.values.length === 2 && def.values.includes("true")) {
    return def.get() === "true" ? "text.success" : "text.dim";
  }
  return "text.dim";
}

// ============================================================================
// Component
// ============================================================================

export function SettingsSelector(props: SettingsSelectorProps) {
  const { preferences, controller, surfaceId, availableWidth, availableHeight } = props;
  const theme = useTheme();
  const [listState, setListState] = createSignal<SelectableListState>(createSelectableListState());
  const selectedIdx = () => listState().selectedIndex;
  const [version, setVersion] = createSignal(0); // reactive tick for preference changes
  const [submenuDef, setSubmenuDef] = createSignal<SettingDef | null>(null);
  const [submenuIdx, setSubmenuIdx] = createSignal(0);

  const defs = createMemo<SettingDef[]>(() => {
    if (!preferences) return [];

    return [
      {
        id: "hide-thinking",
        label: "Hide thinking",
        description: "Hide thinking blocks in assistant responses",
        values: ["true", "false"],
        get: () => (preferences.getHideThinkingBlock() ? "true" : "false"),
        set: (v) => preferences.setHideThinkingBlock(v === "true"),
      },
      {
        id: "theme",
        label: "Theme",
        description: "Color theme for the interface",
        submenu: true,
        get: () => preferences.getTheme() ?? "dark",
        set: (v) => preferences.setTheme(v),
      },
    ];
  });

  // Derive KeyValueItems with reactive version dependency
  const items = createMemo<KeyValueItem[]>(() => {
    version(); // track
    return defs().map((def) => ({
      id: def.id,
      label: def.label,
      value: formatBadge(def) ?? "",
      valueColor: valueColorFor(def),
    }));
  });

  const selectedDesc = createMemo<string>(() => {
    const def = defs()[selectedIdx()];
    return def?.description ?? "";
  });

  const submenuItems = createMemo<SelectItem<null>[]>(() => {
    const def = submenuDef();
    if (!def) return [];
    return getSubmenuOptions(def).map((opt) => ({
      id: opt.value,
      label: opt.label,
      description: opt.description,
      value: null,
    }));
  });

  const openSubmenu = (def: SettingDef) => {
    setSubmenuDef(def);
    const options = getSubmenuOptions(def);
    const current = def.get();
    const idx = options.findIndex((o) => o.value === current);
    setSubmenuIdx(idx >= 0 ? idx : 0);
  };

  const handleSelect = () => {
    const def = defs()[selectedIdx()];
    if (!def) return;
    if (def.submenu) {
      openSubmenu(def);
    } else if (def.values && def.values.length > 0) {
      const current = def.get();
      const idx = def.values.indexOf(current);
      def.set(def.values[(idx + 1) % def.values.length]);
      setVersion((v) => v + 1);
    }
  };

  const confirmSubmenu = () => {
    const def = submenuDef();
    if (!def) return;
    const options = getSubmenuOptions(def);
    const opt = options[submenuIdx()];
    if (opt) {
      def.set(opt.value);
      setVersion((v) => v + 1);
    }
    setSubmenuDef(null);
  };

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): SurfaceKeyResult {
        if (submenuDef()) {
          if (event.name === "escape") {
            setSubmenuDef(null);
            return { type: "handled" };
          }
          if (event.name === "return") {
            confirmSubmenu();
            return { type: "handled" };
          }
          const total = submenuItems().length;
          if (total === 0) return { type: "handled" };
          if (event.name === "up") {
            setSubmenuIdx((i) => (i - 1 + total) % total);
            return { type: "handled" };
          }
          if (event.name === "down") {
            setSubmenuIdx((i) => (i + 1) % total);
            return { type: "handled" };
          }
          return { type: "handled" };
        }
        if (event.name === "space") {
          handleSelect();
          return { type: "handled" };
        }
        const { nextState, result } = selectorBehavior(event, listState(), defs().length);
        setListState(nextState);
        return result;
      },
      onConfirm() {
        if (submenuDef()) confirmSubmenu();
        else handleSelect();
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  // Layout: list + DescriptionBox (hints rendered by shell)
  const descRowCount = () => {
    const d = selectedDesc();
    if (!d) return 0;
    const maxW = Math.max(20, availableWidth - 2);
    let lines = 0;
    let remaining = d;
    while (remaining.length > 0) {
      lines++;
      if (remaining.length <= maxW) break;
      let cut = maxW;
      while (cut > 0 && remaining[cut] !== " ") cut--;
      if (cut === 0) cut = maxW;
      remaining = remaining.slice(cut).trimStart();
    }
    return Math.min(lines, 3);
  };
  const listMaxH = () => Math.max(1, availableHeight - descRowCount() - 1); // desc + blank above

  return (
    <Show
      when={!submenuDef()}
      fallback={
        <box flexDirection="column">
          <box paddingLeft={1} paddingTop={1}>
            <text fg={theme.color("text.accent")}>{`  ${submenuDef()!.label}`}</text>
          </box>
          <SelectListView
            items={submenuItems()}
            selectedIndex={submenuIdx()}
            width={availableWidth}
            maxHeight={Math.min(submenuItems().length + 2, 8)}
            showDescriptions={true}
            itemSpacing={0}
            onSelect={() => {}}
          />
          <HintBar hints="Enter to select  Esc to go back" />
        </box>
      }
    >
      <box flexDirection="column">
        <KeyValueList
          items={items()}
          selectedIndex={selectedIdx()}
          maxVisible={listMaxH()}
          width={availableWidth}
        />
        <DescriptionBox text={selectedDesc()} width={availableWidth} />
      </box>
    </Show>
  );
}
