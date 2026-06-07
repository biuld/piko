// ============================================================================
// Settings Selector — pi-style flat list with value badges + description area.
//
// Each item: single-line (label + value badge). Description of selected item
// shown in a panel below. Bool/enum values cycle on Enter/Space.
// Submenus for thinking level and theme.
// ============================================================================

import { createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import type { PikoHost, SettingsManager } from "piko-host-runtime";
import type { TuiStore } from "../store.js";
import type { SelectItem } from "./selector-controller.js";
import { SelectListView } from "./SelectListView.js";
import type { TuiController } from "../../../runtime/tui-controller.js";
import type { KeyEvent } from "../../../focus/types.js";
import { menuBehavior, type SurfaceKeyResult } from "../../../surfaces/index.js";
import { useTheme } from "../theme-context.js";
import { truncateToWidth } from "../../../layout/measure.js";

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

// ============================================================================
// Component
// ============================================================================

export function SettingsSelector(props: SettingsSelectorProps) {
  const { store, settingsManager: sm, host, controller, surfaceId } = props;
  const theme = useTheme();
  const [selectedIdx, setSelectedIdx] = createSignal(0);
  const [submenuDef, setSubmenuDef] = createSignal<SettingDef | null>(null);
  const [submenuIdx, setSubmenuIdx] = createSignal(0);

  // =========================================================================
  // Setting definitions
  // =========================================================================

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
        description:
          "Enter while streaming queues steering messages. 'one-at-a-time': deliver one, wait for response. 'all': deliver all at once.",
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
        description:
          "Queued follow-up messages until agent stops. 'one-at-a-time': deliver one, wait for response. 'all': deliver all at once.",
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

  // =========================================================================
  // Derive items
  // =========================================================================

  const items = createMemo<SelectItem<SettingDef>[]>(() =>
    defs().map((def) => ({
      id: def.id,
      label: def.label,
      badge: formatBadge(def),
      value: def,
    })),
  );

  // Description of selected item
  const selectedDesc = createMemo<string>(() => {
    const def = defs()[selectedIdx()];
    return def?.description ?? "";
  });

  // =========================================================================
  // Submenu
  // =========================================================================

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

  // =========================================================================
  // Actions
  // =========================================================================

  const handleSelect = () => {
    const sel = items()[selectedIdx()];
    if (!sel?.value) return;
    const def = sel.value;

    if (def.submenu) {
      openSubmenu(def);
    } else if (def.values && def.values.length > 0) {
      const current = def.get();
      const idx = def.values.indexOf(current);
      const next = def.values[(idx + 1) % def.values.length];
      def.set(next);
    }
  };

  const confirmSubmenu = () => {
    const def = submenuDef();
    if (!def) return;
    const options = getSubmenuOptions(def);
    const opt = options[submenuIdx()];
    if (opt) def.set(opt.value);
    setSubmenuDef(null);
  };

  // =========================================================================
  // Keyboard
  // =========================================================================

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
          // Space also toggles for submenus? No — just Enter/arrows.
          return { type: "handled" };
        }

        // Space toggles value (same as Enter for settings)
        if (event.name === "space") {
          handleSelect();
          return { type: "handled" };
        }

        const listState = { query: "", selectedIndex: selectedIdx() };
        const { nextState, result } = menuBehavior(event, listState, defs().length);
        setSelectedIdx(nextState.selectedIndex);
        return result;
      },
      onConfirm() {
        if (submenuDef()) confirmSubmenu();
        else handleSelect();
      },
    });
  });

  onCleanup(() => controller.setSurfaceController(surfaceId, null));

  // =========================================================================
  // Layout
  // =========================================================================

  const surface = () => controller.store.state().surfaces.find((s) => s.id === surfaceId);
  const placement = () => surface()?.placement ?? "partial";
  const viewportHeight = () => controller.store.state().layout.viewport.height;
  const termWidth = () => store.state().layout.viewport.width;
  const descLines = () => {
    const desc = selectedDesc();
    if (!desc) return [];
    // Simple word-wrap: split into chunks of termWidth - 4
    const maxW = Math.max(20, termWidth() - 4);
    const lines: string[] = [];
    let remaining = desc;
    while (remaining.length > 0) {
      if (remaining.length <= maxW) {
        lines.push(remaining);
        break;
      }
      // Find last space within maxW
      let cut = maxW;
      while (cut > 0 && remaining[cut] !== " ") cut--;
      if (cut === 0) cut = maxW; // no space found, hard break
      lines.push(remaining.slice(0, cut));
      remaining = remaining.slice(cut).trimStart();
    }
    return lines;
  };

  const listMaxHeight = () => {
    const base = submenuDef() ? 12 : Math.min(defs().length + 2, 18);
    if (props.maxHeight !== undefined) return props.maxHeight;
    if (placement() === "full") {
      return Math.max(10, viewportHeight() - 8);
    }
    return base;
  };

  // =========================================================================
  // Render
  // =========================================================================

  return (
    <Show
      when={!submenuDef()}
      fallback={
        <box flexDirection="column">
          <box padding={1}>
            <text fg={theme.color("text.accent")}>
              {`  ${submenuDef()!.label}`}
            </text>
          </box>
          <SelectListView
            items={submenuItems()}
            selectedIndex={submenuIdx()}
            width={termWidth()}
            maxHeight={8}
            showDescriptions={true}
            itemSpacing={0}
            onSelect={() => {}}
          />
          <box padding={1}>
            <text fg={theme.color("text.dim")}>  Enter to select · Esc to go back</text>
          </box>
        </box>
      }
    >
      <box flexDirection="column">
        {/* Settings list */}
        <SelectListView
          items={items()}
          selectedIndex={selectedIdx()}
          width={termWidth()}
          maxHeight={listMaxHeight()}
          showDescriptions={false}
          itemSpacing={0}
          onSelect={() => {}}
        />

        {/* Description of selected item */}
        <Show when={descLines().length > 0}>
          <box flexDirection="column" paddingTop={1} paddingLeft={1}>
            {descLines().map((line) => (
              <text fg={theme.color("text.dim")}>{`  ${line}`}</text>
            ))}
          </box>
        </Show>

        {/* Hint line */}
        <box paddingTop={1} paddingLeft={1}>
          <text fg={theme.color("text.dim")}>
            {`  Enter/Space to change · Esc to close`}
          </text>
        </box>
      </box>
    </Show>
  );
}
