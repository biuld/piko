/**
 * Settings selector overlay — rich settings panel with submenus.
 *
 * Uses SettingsList from pi-tui for navigation and submenu support.
 * Mirrors pi's settings selector with:
 * - Boolean toggles (compaction, retry, hide thinking, etc.)
 * - Multi-value cycling (transport, etc.)
 * - Submenus for thinking level and theme selection
 * - Theme preview on hover
 * - Search support
 */

import {
  Container,
  type SelectItem,
  SelectList,
  type SettingItem,
  SettingsList,
  Spacer,
  Text,
} from "@earendil-works/pi-tui";
import type { SettingsManager } from "piko-host-runtime";
import { DynamicBorder } from "../components/dynamic-border.js";
import { getThemeManager } from "../theme/index.js";
import { getSelectListTheme, getSettingsListTheme, getTheme } from "../theme.js";
import { makeFocusable } from "./focusable.js";
import type { OverlayContext } from "./index.js";

// ============================================================================
// Types
// ============================================================================

export interface SettingsCallbacks {
  onThemePreview?: (themeName: string) => void;
  onThemeChange?: (themeName: string) => void;
}

// ============================================================================
// Submenu: SelectSubmenu
// ============================================================================

class SelectSubmenu extends Container {
  private selectList: SelectList;

  constructor(
    title: string,
    description: string,
    options: SelectItem[],
    currentValue: string,
    onSelect: (value: string) => void,
    onCancel: () => void,
    onSelectionChange?: (value: string) => void,
  ) {
    super();
    const t = getTheme();

    // Title
    this.addChild(new Text(t.fg("accent", t.bold(title)), 0, 0));

    // Description
    if (description) {
      this.addChild(new Spacer(1));
      this.addChild(new Text(t.fg("muted", description), 0, 0));
    }

    // Spacer
    this.addChild(new Spacer(1));

    // Select list
    this.selectList = new SelectList(options, Math.min(options.length, 10), getSelectListTheme());

    // Pre-select current value
    const currentIndex = options.findIndex((o) => o.value === currentValue);
    if (currentIndex !== -1) {
      this.selectList.setSelectedIndex(currentIndex);
    }

    this.selectList.onSelect = (item) => {
      onSelect(item.value);
    };

    this.selectList.onCancel = onCancel;

    if (onSelectionChange) {
      this.selectList.onSelectionChange = (item) => {
        onSelectionChange(item.value);
      };
    }

    this.addChild(this.selectList);

    // Hint
    this.addChild(new Spacer(1));
    this.addChild(new Text(t.fg("dim", "  Enter to select · Esc to go back"), 0, 0));
  }

  handleInput(data: string): void {
    this.selectList.handleInput(data);
  }
}

// ============================================================================
// Thinking level descriptions
// ============================================================================

const THINKING_DESCRIPTIONS: Record<string, string> = {
  off: "No reasoning",
  minimal: "Very brief reasoning (~1k tokens)",
  low: "Light reasoning (~2k tokens)",
  medium: "Moderate reasoning (~8k tokens)",
  high: "Deep reasoning (~16k tokens)",
  xhigh: "Maximum reasoning (~32k tokens)",
};

const THINKING_LEVELS = ["off", "minimal", "low", "medium", "high", "xhigh"] as const;

// ============================================================================
// Main entry point
// ============================================================================

export async function openSettingsSelector(
  ctx: OverlayContext,
  settingsManager: SettingsManager,
  callbacks?: SettingsCallbacks,
): Promise<void> {
  const t = getTheme();
  const tm = getThemeManager();
  const availableThemes = tm.list();
  const currentThemeName = tm.getCurrentName();

  // ---- Build setting items ----

  function buildItems(): SettingItem[] {
    const s = settingsManager.settings;

    const items: SettingItem[] = [
      {
        id: "compaction",
        label: "Auto-compact",
        description: "Automatically compact context when it gets too large",
        currentValue: (s.compaction?.enabled ?? true) ? "true" : "false",
        values: ["true", "false"],
      },
      {
        id: "retry",
        label: "Retry on error",
        description: "Automatically retry failed LLM requests",
        currentValue: (s.retry?.enabled ?? true) ? "true" : "false",
        values: ["true", "false"],
      },
      {
        id: "hide-thinking",
        label: "Hide thinking",
        description: "Hide thinking blocks in assistant responses",
        currentValue: (s.hideThinkingBlock ?? false) ? "true" : "false",
        values: ["true", "false"],
      },
      {
        id: "thinking",
        label: "Thinking level",
        description: "Reasoning depth for thinking-capable models",
        currentValue: s.defaultThinkingLevel ?? "off",
        submenu: (currentValue, done) =>
          new SelectSubmenu(
            "Thinking Level",
            "Select reasoning depth for thinking-capable models",
            THINKING_LEVELS.map((level) => ({
              value: level,
              label: level,
              description: THINKING_DESCRIPTIONS[level],
            })),
            currentValue,
            (value) => {
              settingsManager.setDefaultThinkingLevel(
                value as "off" | "minimal" | "low" | "medium" | "high" | "xhigh",
              );
              done(value);
            },
            () => done(),
          ),
      },
      {
        id: "theme",
        label: "Theme",
        description: "Color theme for the interface",
        currentValue: currentThemeName,
        submenu: (currentValue, done) =>
          new SelectSubmenu(
            "Theme",
            "Select color theme",
            availableThemes.map((name) => ({
              value: name,
              label: name,
            })),
            currentValue,
            (value) => {
              if (tm.switchTo(value)) {
                settingsManager.setTheme(value);
                callbacks?.onThemeChange?.(value);
              }
              done(value);
            },
            () => {
              // Restore original theme on cancel
              const originalTheme = settingsManager.getTheme() ?? "dark";
              if (tm.switchTo(originalTheme)) {
                callbacks?.onThemePreview?.(originalTheme);
              }
              done();
            },
            (value) => {
              // Preview theme on selection change
              if (tm.switchTo(value)) {
                callbacks?.onThemePreview?.(value);
                ctx.tui.requestRender();
              }
            },
          ),
      },
      {
        id: "sessiondir",
        label: "Session directory",
        description: "Custom directory for session storage",
        currentValue: s.sessionDir ?? "default",
        values: undefined, // read-only display
      },
    ];

    return items;
  }

  return new Promise<void>((resolve) => {
    const items = buildItems();
    const settingsList = new SettingsList(
      items,
      10,
      getSettingsListTheme(),
      (id, newValue) => {
        switch (id) {
          case "compaction":
            settingsManager.setCompactionEnabled(newValue === "true");
            break;
          case "retry":
            settingsManager.setRetryEnabled(newValue === "true");
            break;
          case "hide-thinking":
            settingsManager.setHideThinkingBlock(newValue === "true");
            break;
        }
        // Update item value in-place for toggle settings
        settingsList.updateValue(id, newValue);
      },
      () => {
        replacementHandle?.hide();
        resolve();
      },
      { enableSearch: true },
    );

    const container = new Container();
    container.addChild(new DynamicBorder((s: string) => t.fg("border", s)));
    container.addChild(new Text(t.fg("accent", t.bold(" Settings")), 1, 0));
    container.addChild(new Spacer(1));
    container.addChild(settingsList);
    container.addChild(new DynamicBorder((s: string) => t.fg("border", s)));

    const component = makeFocusable(container);
    Object.assign(component, {
      handleInput(data: string) {
        settingsList.handleInput(data);
        ctx.tui.requestRender();
      },
    });

    const replacementHandle = ctx.showReplacement(component);
    // Store reference for cancel handler
    const _replacementHandle = replacementHandle;
  });
}
