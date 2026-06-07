// ============================================================================
// Settings Selector — pi-style flat settings list with inline value cycling.
//
// Each setting shows: label, current value (badge), description (meta line).
// Bool/enum values cycle on Enter. Complex settings (thinking, theme) open
// a submenu selector. No inline text editing.
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

// ============================================================================
// Types
// ============================================================================

interface SettingDef {
  id: string;
  label: string;
  description: string;
  /** Predefined values to cycle through. If set, Enter cycles values. */
  values?: string[];
  /** Current value getter (returns display string). */
  get: () => string;
  /** Called when value changes (receives the new display string). */
  set: (value: string) => void;
  /** If true, Enter opens a submenu instead of cycling. */
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
  if (def.submenu) return `${def.get()}  \u203a`;
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

    const result: SettingDef[] = [];

    result.push({
      id: "autocompact",
      label: "Auto-compact",
      description: "Automatically compact context when it gets too large",
      values: ["true", "false"],
      get: () => (sm.getCompactionSettings().enabled ? "true" : "false"),
      set: (v) => sm.setCompactionEnabled(v === "true"),
    });

    result.push({
      id: "steering-mode",
      label: "Steering mode",
      description:
        "Enter while streaming queues steering messages. 'one-at-a-time': deliver one, wait. 'all': deliver all at once.",
      values: ["one-at-a-time", "all"],
      get: () => sm.getSteeringMode(),
      set: (v) => {
        sm.setSteeringMode(v as "all" | "one-at-a-time");
        host?.setSteeringMode(v as "all" | "one-at-a-time");
      },
    });

    result.push({
      id: "follow-up-mode",
      label: "Follow-up mode",
      description:
        "Queued follow-up messages until agent stops. 'one-at-a-time': deliver one, wait. 'all': deliver all at once.",
      values: ["one-at-a-time", "all"],
      get: () => sm.getFollowUpMode(),
      set: (v) => {
        sm.setFollowUpMode(v as "all" | "one-at-a-time");
        host?.setFollowUpMode(v as "all" | "one-at-a-time");
      },
    });

    result.push({
      id: "transport",
      label: "Transport",
      description: "Preferred transport for providers that support multiple transports",
      values: ["auto", "stdio", "sse"],
      get: () => sm.getTransport(),
      set: (v) => sm.setTransport(v as any),
    });

    result.push({
      id: "hide-thinking",
      label: "Hide thinking",
      description: "Hide thinking blocks in assistant responses",
      values: ["true", "false"],
      get: () => (sm.getHideThinkingBlock() ? "true" : "false"),
      set: (v) => sm.setHideThinkingBlock(v === "true"),
    });

    result.push({
      id: "quiet-startup",
      label: "Quiet startup",
      description: "Disable verbose printing at startup",
      values: ["true", "false"],
      get: () => (sm.getQuietStartup() ? "true" : "false"),
      set: (v) => sm.setQuietStartup(v === "true"),
    });

    result.push({
      id: "double-escape-action",
      label: "Double-escape action",
      description: "Action when pressing Escape twice with empty editor",
      values: ["tree", "fork", "none"],
      get: () => sm.getDoubleEscapeAction(),
      set: (v) => sm.setDoubleEscapeAction(v as "tree" | "fork" | "none"),
    });

    result.push({
      id: "retry",
      label: "Retry on failure",
      description: "Automatically retry after LLM provider errors",
      values: ["true", "false"],
      get: () => (sm.getRetrySettings().enabled ? "true" : "false"),
      set: (v) => sm.setRetryEnabled(v === "true"),
    });

    result.push({
      id: "max-retries",
      label: "Max retries",
      description: "Maximum consecutive retry attempts before giving up",
      values: ["1", "2", "3", "5", "10"],
      get: () => String(sm.getRetrySettings().maxRetries),
      set: (v) => sm.setRetryMaxRetries(Number(v)),
    });

    result.push({
      id: "compaction-reserve",
      label: "Compaction reserve",
      description: "Tokens reserved for the system prompt and output buffer",
      values: ["4096", "8192", "16384", "32768", "65536"],
      get: () => String(sm.getCompactionSettings().reserveTokens),
      set: (v) => sm.setCompactionReserveTokens(Number(v)),
    });

    result.push({
      id: "compaction-keep-recent",
      label: "Keep recent tokens",
      description: "Tokens to always keep from recent conversation turns",
      values: ["4096", "8192", "16384", "20000", "32768"],
      get: () => String(sm.getCompactionSettings().keepRecentTokens),
      set: (v) => sm.setCompactionKeepRecentTokens(Number(v)),
    });

    result.push({
      id: "clear-on-shrink",
      label: "Clear on shrink",
      description: "Clear empty rows when content shrinks (may cause flicker)",
      values: ["true", "false"],
      get: () => (sm.getClearOnShrink() ? "true" : "false"),
      set: (v) => sm.setClearOnShrink(v === "true"),
    });

    result.push({
      id: "thinking",
      label: "Thinking level",
      description: "Reasoning depth for thinking-capable models",
      submenu: true,
      get: () => sm.settings.defaultThinkingLevel ?? "off",
      set: (v) => sm.setDefaultThinkingLevel(v as any),
    });

    result.push({
      id: "theme",
      label: "Theme",
      description: "Color theme for the interface",
      submenu: true,
      get: () => sm.getTheme() ?? "dark",
      set: (v) => sm.setTheme(v),
    });

    return result;
  });

  // =========================================================================
  // Derive items for SelectListView
  // =========================================================================

  const items = createMemo<SelectItem<SettingDef>[]>(() =>
    defs().map((def) => ({
      id: def.id,
      label: def.label,
      badge: formatBadge(def),
      meta: def.description,
      value: def,
    })),
  );

  const itemCount = () => items().length;

  // =========================================================================
  // Submenu items
  // =========================================================================

  const submenuItems = createMemo<SelectItem<null>[]>(() => {
    const def = submenuDef();
    if (!def) return [];
    const options = getSubmenuOptions(def);
    return options.map((opt) => ({
      id: opt.value,
      label: opt.label,
      meta: opt.description,
      value: null,
    }));
  });

  // When opening a submenu, set initial index to current value
  const openSubmenu = (def: SettingDef) => {
    setSubmenuDef(def);
    const options = getSubmenuOptions(def);
    const current = def.get();
    const idx = options.findIndex((o) => o.value === current);
    setSubmenuIdx(idx >= 0 ? idx : 0);
  };

  // =========================================================================
  // Handle selection
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

  // =========================================================================
  // Submenu select
  // =========================================================================

  const confirmSubmenu = () => {
    const def = submenuDef();
    if (!def) return;
    const options = getSubmenuOptions(def);
    const opt = options[submenuIdx()];
    if (opt) {
      def.set(opt.value);
    }
    setSubmenuDef(null);
  };

  // =========================================================================
  // Keyboard
  // =========================================================================

  onMount(() => {
    controller.setSurfaceController(surfaceId, {
      handleKey(event: KeyEvent): SurfaceKeyResult {
        // Submenu mode
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

        // Normal navigation
        const listState = { query: "", selectedIndex: selectedIdx() };
        const { nextState, result } = menuBehavior(event, listState, itemCount());
        setSelectedIdx(nextState.selectedIndex);
        return result;
      },
      onConfirm() {
        if (submenuDef()) {
          confirmSubmenu();
        } else {
          handleSelect();
        }
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

  const maxHeight = () => {
    if (submenuDef()) return Math.min(submenuItems().length + 3, 12);
    if (props.maxHeight !== undefined) return props.maxHeight;
    if (placement() === "full") {
      return Math.max(15, viewportHeight() - 6);
    }
    return Math.min(itemCount() * 2 + 2, 22);
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
            width={store.state().layout.viewport.width}
            maxHeight={maxHeight()}
            showDescriptions={false}
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
        {items().length > 0 ? (
          <SelectListView
            items={items()}
            selectedIndex={selectedIdx()}
            width={store.state().layout.viewport.width}
            maxHeight={maxHeight()}
            showDescriptions={false}
            itemSpacing={0}
            onSelect={() => {}}
          />
        ) : (
          <box padding={1}>
            <text>No settings available</text>
          </box>
        )}
      </box>
    </Show>
  );
}
