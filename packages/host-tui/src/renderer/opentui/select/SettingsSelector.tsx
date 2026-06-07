// ============================================================================
// Settings Selector — ListBody + DescriptionBox + HintBar.
//
// Self-contained: owns all state, keyboard handling, and UI composition.
// Bool/enum values cycle on Enter/Space. Submenus for thinking/theme.
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import type { PikoHost, SettingsManager } from "piko-host-runtime";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import { createSelectableListState, type SelectableListState } from "../../../surfaces/interactions/selectable-list.js";
import { selectorBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";
import { KeyValueList, DescriptionBox, HintBar } from "../primitives/index.js";
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
  settingsManager?: SettingsManager;
  host?: PikoHost;
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
  if (def.id === "thinking") {
    return ["off", "minimal", "low", "medium", "high", "xhigh"].map((level) => ({
      value: level,
      label: level,
      description:
        {
          off: "No reasoning",
          minimal: "Very brief (~1k tokens)",
          low: "Light (~2k tokens)",
          medium: "Moderate (~8k tokens)",
          high: "Deep (~16k tokens)",
          xhigh: "Maximum (~32k tokens)",
        }[level] ?? "",
    }));
  }
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
  const { settingsManager: sm, host, controller, surfaceId, availableWidth, availableHeight, onClose } = props;
  const theme = useTheme();
  const [listState, setListState] = createSignal<SelectableListState>(
    createSelectableListState(),
  );
  const selectedIdx = () => listState().selectedIndex;
  const [version, setVersion] = createSignal(0); // reactive tick for SettingsManager changes
  const [submenuDef, setSubmenuDef] = createSignal<SettingDef | null>(null);
  const [submenuIdx, setSubmenuIdx] = createSignal(0);

  const defs = createMemo<SettingDef[]>(() => {
    if (!sm) return [];

    return [
      {
        id: "autocompact",
        label: "Auto-compact",
        description: "Automatically compact context when it gets too large",
        values: ["true", "false"],
        get: () => (sm.getCompactionSettings().enabled ? "true" : "false"),
        set: (v) => sm.setCompactionEnabled(v === "true"),
      },
      {
        id: "steering-mode",
        label: "Steering mode",
        description: "Enter while streaming queues steering messages. 'one-at-a-time': deliver one, wait. 'all': deliver all at once.",
        values: ["one-at-a-time", "all"],
        get: () => sm.getSteeringMode(),
        set: (v) => {
          sm.setSteeringMode(v as "all" | "one-at-a-time");
          host?.setSteeringMode(v as "all" | "one-at-a-time");
        },
      },
      {
        id: "follow-up-mode",
        label: "Follow-up mode",
        description: "Queued follow-up messages until agent stops. 'one-at-a-time': deliver one, wait. 'all': deliver all at once.",
        values: ["one-at-a-time", "all"],
        get: () => sm.getFollowUpMode(),
        set: (v) => {
          sm.setFollowUpMode(v as "all" | "one-at-a-time");
          host?.setFollowUpMode(v as "all" | "one-at-a-time");
        },
      },
      {
        id: "transport",
        label: "Transport",
        description: "Preferred transport for providers that support multiple transports",
        values: ["auto", "stdio", "sse"],
        get: () => sm.getTransport(),
        set: (v) => sm.setTransport(v as any),
      },
      {
        id: "hide-thinking",
        label: "Hide thinking",
        description: "Hide thinking blocks in assistant responses",
        values: ["true", "false"],
        get: () => (sm.getHideThinkingBlock() ? "true" : "false"),
        set: (v) => sm.setHideThinkingBlock(v === "true"),
      },
      {
        id: "quiet-startup",
        label: "Quiet startup",
        description: "Disable verbose printing at startup",
        values: ["true", "false"],
        get: () => (sm.getQuietStartup() ? "true" : "false"),
        set: (v) => sm.setQuietStartup(v === "true"),
      },
      {
        id: "double-escape-action",
        label: "Double-escape action",
        description: "Action when pressing Escape twice with empty editor",
        values: ["tree", "fork", "none"],
        get: () => sm.getDoubleEscapeAction(),
        set: (v) => sm.setDoubleEscapeAction(v as "tree" | "fork" | "none"),
      },
      {
        id: "retry",
        label: "Retry on failure",
        description: "Automatically retry after LLM provider errors",
        values: ["true", "false"],
        get: () => (sm.getRetrySettings().enabled ? "true" : "false"),
        set: (v) => sm.setRetryEnabled(v === "true"),
      },
      {
        id: "max-retries",
        label: "Max retries",
        description: "Maximum consecutive retry attempts before giving up",
        values: ["1", "2", "3", "5", "10"],
        get: () => String(sm.getRetrySettings().maxRetries),
        set: (v) => sm.setRetryMaxRetries(Number(v)),
      },
      {
        id: "compaction-reserve",
        label: "Compaction reserve",
        description: "Tokens reserved for the system prompt and output buffer",
        values: ["4096", "8192", "16384", "32768", "65536"],
        get: () => String(sm.getCompactionSettings().reserveTokens),
        set: (v) => sm.setCompactionReserveTokens(Number(v)),
      },
      {
        id: "compaction-keep-recent",
        label: "Keep recent tokens",
        description: "Tokens to always keep from recent conversation turns",
        values: ["4096", "8192", "16384", "20000", "32768"],
        get: () => String(sm.getCompactionSettings().keepRecentTokens),
        set: (v) => sm.setCompactionKeepRecentTokens(Number(v)),
      },
      {
        id: "clear-on-shrink",
        label: "Clear on shrink",
        description: "Clear empty rows when content shrinks (may cause flicker)",
        values: ["true", "false"],
        get: () => (sm.getClearOnShrink() ? "true" : "false"),
        set: (v) => sm.setClearOnShrink(v === "true"),
      },
      {
        id: "thinking",
        label: "Thinking level",
        description: "Reasoning depth for thinking-capable models",
        submenu: true,
        get: () => sm.settings.defaultThinkingLevel ?? "off",
        set: (v) => sm.setDefaultThinkingLevel(v as any),
      },
      {
        id: "theme",
        label: "Theme",
        description: "Color theme for the interface",
        submenu: true,
        get: () => sm.getTheme() ?? "dark",
        set: (v) => sm.setTheme(v),
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
    if (def.submenu) { openSubmenu(def); }
    else if (def.values && def.values.length > 0) {
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
    if (opt) { def.set(opt.value); setVersion((v) => v + 1); }
    setSubmenuDef(null);
  };

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): SurfaceKeyResult {
        if (submenuDef()) {
          if (event.name === "escape") { setSubmenuDef(null); return { type: "handled" }; }
          if (event.name === "return") { confirmSubmenu(); return { type: "handled" }; }
          const total = submenuItems().length;
          if (total === 0) return { type: "handled" };
          if (event.name === "up") { setSubmenuIdx((i) => (i - 1 + total) % total); return { type: "handled" }; }
          if (event.name === "down") { setSubmenuIdx((i) => (i + 1) % total); return { type: "handled" }; }
          return { type: "handled" };
        }
        if (event.name === "space") { handleSelect(); return { type: "handled" }; }
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
  const listMaxH = () => Math.max(1, availableHeight - descRowCount());

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
