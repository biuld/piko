/**
 * Theme Manager — loads themes from JSON files and manages theme switching.
 *
 * Supports:
 * - Built-in themes (default dark/light)
 * - External JSON themes from .piko/themes/
 * - Runtime theme switching
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { getPikoDir } from "piko-host-runtime";
import { Theme } from "../theme.js";

// ============================================================================
// Types
// ============================================================================

export interface ThemeDef {
  name: string;
  path?: string;
  colors: Record<string, string>;
}

// ============================================================================
// Built-in themes
// ============================================================================

const DARK_COLORS: Record<string, string> = {
  accent: "#8abeb7",
  border: "#5f87ff",
  borderAccent: "#00d7ff",
  borderMuted: "#505050",
  success: "#b5bd68",
  error: "#cc6666",
  warning: "#ffff00",
  muted: "#808080",
  dim: "#666666",
  text: "#d4d4d4",
  thinkingText: "#808080",
  selectedBg: "#3a3a4a",
  userMessageBg: "#343541",
  userMessageText: "#d4d4d4",
  customMessageBg: "#2d2838",
  customMessageText: "#d4d4d4",
  customMessageLabel: "#9575cd",
  toolPendingBg: "#282832",
  toolSuccessBg: "#283228",
  toolErrorBg: "#3c2828",
  toolTitle: "#d4d4d4",
  toolOutput: "#808080",
  mdHeading: "#f0c674",
  mdLink: "#81a2be",
  mdLinkUrl: "#666666",
  mdCode: "#8abeb7",
  mdCodeBlock: "#b5bd68",
  mdCodeBlockBorder: "#808080",
  mdQuote: "#808080",
  mdQuoteBorder: "#808080",
  mdHr: "#808080",
  mdListBullet: "#8abeb7",
  toolDiffAdded: "#b5bd68",
  toolDiffRemoved: "#cc6666",
  toolDiffContext: "#808080",
  syntaxComment: "#6A9955",
  syntaxKeyword: "#569CD6",
  syntaxFunction: "#DCDCAA",
  syntaxVariable: "#9CDCFE",
  syntaxString: "#CE9178",
  syntaxNumber: "#B5CEA8",
  syntaxType: "#4EC9B0",
  syntaxOperator: "#D4D4D4",
  syntaxPunctuation: "#D4D4D4",
  thinkingOff: "#505050",
  thinkingMinimal: "#6e6e6e",
  thinkingLow: "#5f87af",
  thinkingMedium: "#81a2be",
  thinkingHigh: "#b294bb",
  thinkingXhigh: "#d183e8",
  bashMode: "#b5bd68",
};

const LIGHT_COLORS: Record<string, string> = {
  ...Object.fromEntries(Object.keys(DARK_COLORS).map((k) => [k, ""])),
  accent: "#2563eb",
  border: "#64748b",
  borderAccent: "#2563eb",
  borderMuted: "#cbd5e1",
  success: "#16a34a",
  error: "#dc2626",
  warning: "#d4a017",
  muted: "#64748b",
  dim: "#94a3b8",
  text: "#1e293b",
  thinkingText: "#64748b",
  selectedBg: "#dbeafe",
  userMessageBg: "#f8fafc",
  userMessageText: "#1e293b",
  customMessageBg: "#f0f4ff",
  customMessageText: "#1e293b",
  customMessageLabel: "#7c3aed",
  toolPendingBg: "#f1f5f9",
  toolSuccessBg: "#f0fdf4",
  toolErrorBg: "#fef2f2",
  toolTitle: "#1e293b",
  toolOutput: "#64748b",
  mdHeading: "#b45309",
  mdLink: "#2563eb",
  mdLinkUrl: "#64748b",
  mdCode: "#059669",
  mdCodeBlock: "#166534",
  mdCodeBlockBorder: "#cbd5e1",
  mdQuote: "#64748b",
  mdQuoteBorder: "#cbd5e1",
  mdHr: "#cbd5e1",
  mdListBullet: "#2563eb",
  toolDiffAdded: "#16a34a",
  toolDiffRemoved: "#dc2626",
  toolDiffContext: "#64748b",
  syntaxComment: "#6a9955",
  syntaxKeyword: "#2563eb",
  syntaxFunction: "#9333ea",
  syntaxVariable: "#0891b2",
  syntaxString: "#16a34a",
  syntaxNumber: "#0891b2",
  syntaxType: "#7c3aed",
  syntaxOperator: "#1e293b",
  syntaxPunctuation: "#1e293b",
  thinkingOff: "#94a3b8",
  thinkingMinimal: "#64748b",
  thinkingLow: "#2563eb",
  thinkingMedium: "#7c3aed",
  thinkingHigh: "#db2777",
  thinkingXhigh: "#dc2626",
  bashMode: "#16a34a",
};

const _BUILTIN_THEMES: Record<string, Record<string, string>> = {
  dark: DARK_COLORS,
  light: LIGHT_COLORS,
};

// ============================================================================
// Theme Manager
// ============================================================================

export class ThemeManager {
  private themes = new Map<string, ThemeDef>();
  private currentName: string;
  private currentTheme: Theme | null = null;
  private onChangeCallbacks: Array<() => void> = [];

  constructor() {
    this.themes.set("dark", { name: "dark", colors: DARK_COLORS });
    this.themes.set("light", { name: "light", colors: LIGHT_COLORS });
    this.currentName = "dark";
  }

  /** Load themes from built-in + .piko/themes/ (project + global). */
  load(cwd?: string): string[] {
    // Load global themes
    const globalDir = join(getPikoDir(), "themes");
    this.loadFromDir(globalDir);

    // Load project themes
    if (cwd) {
      const projectDir = resolve(cwd, ".piko", "themes");
      this.loadFromDir(projectDir);
    }

    return this.list();
  }

  private loadFromDir(dir: string): void {
    if (!existsSync(dir)) return;

    try {
      const entries = readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.name.endsWith(".json")) continue;

        const fullPath = join(dir, entry.name);
        const isFile = entry.isFile() || (entry.isSymbolicLink() && statSync(fullPath).isFile());
        if (!isFile) continue;

        try {
          const content = readFileSync(fullPath, "utf-8");
          const json = JSON.parse(content);

          if (json.name && json.colors && typeof json.colors === "object") {
            this.themes.set(json.name, {
              name: json.name,
              path: fullPath,
              colors: json.colors,
            });
          }
        } catch {
          // Skip invalid theme files
        }
      }
    } catch {
      // Skip unreadable dirs
    }
  }

  list(): string[] {
    return Array.from(this.themes.keys()).sort();
  }

  get(name?: string): Theme {
    const themeName = name ?? this.currentName;
    const def = this.themes.get(themeName) ?? this.themes.get("dark")!;

    if (!this.currentTheme || this.currentName !== themeName) {
      this.currentTheme = new Theme(def.colors);
      this.currentName = themeName;
    }

    return this.currentTheme;
  }

  switchTo(name: string): boolean {
    if (!this.themes.has(name)) return false;

    this.currentName = name;
    this.currentTheme = new Theme(this.themes.get(name)!.colors);

    for (const cb of this.onChangeCallbacks) {
      cb();
    }

    return true;
  }

  onChange(cb: () => void): void {
    this.onChangeCallbacks.push(cb);
  }

  getCurrentName(): string {
    return this.currentName;
  }
}

// ============================================================================
// Singleton
// ============================================================================

let themeManager: ThemeManager | null = null;

export function getThemeManager(): ThemeManager {
  if (!themeManager) {
    themeManager = new ThemeManager();
  }
  return themeManager;
}
