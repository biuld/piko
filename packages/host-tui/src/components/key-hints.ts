import { getKeybindings, type Keybinding } from "@earendil-works/pi-tui";
import { getTheme } from "../theme.js";

/**
 * Context-aware keybinding hints.
 *
 * Dynamically shows available shortcuts based on current app state:
 * - normal: editing prompt
 * - streaming: active run (abort available)
 * - overlay: selector/dialog open
 * - tree: session tree navigation
 * - branch: branch summary view
 */

export type AppContext = "normal" | "streaming" | "overlay" | "tree" | "branch";

export interface KeyHintConfig {
  key: string;
  label: string;
  context: AppContext | AppContext[];
}

const DEFAULT_HINTS: KeyHintConfig[] = [
  // Normal mode
  { key: "Enter", label: "submit", context: "normal" },
  { key: "Ctrl+D", label: "exit", context: "normal" },
  { key: "Ctrl+C", label: "clear/quit", context: "normal" },
  { key: "Ctrl+P", label: "prev model", context: "normal" },
  { key: "Ctrl+N", label: "next model", context: "normal" },
  { key: "Ctrl+T", label: "theme", context: "normal" },
  // Streaming mode
  { key: "Ctrl+C", label: "abort", context: "streaming" },
  // Tree navigation
  { key: "↑↓", label: "navigate", context: "tree" },
  { key: "Enter", label: "select", context: "tree" },
  { key: "Tab", label: "scope", context: "tree" },
  { key: "Ctrl+R", label: "rename", context: "tree" },
  { key: "Ctrl+D", label: "delete", context: "tree" },
  { key: "Esc", label: "cancel", context: "tree" },
  // Overlay mode
  { key: "↑↓", label: "navigate", context: "overlay" },
  { key: "Enter", label: "confirm", context: "overlay" },
  { key: "Esc", label: "cancel", context: "overlay" },
  // Branch summary
  { key: "Esc", label: "close", context: "branch" },
  { key: "Enter", label: "resume", context: "branch" },
];

/**
 * Format a keybinding as human-readable text with themed dim/muted styling.
 */
export function keyHint(keybinding: Keybinding, description: string): string {
  const t = getTheme();
  const keys = getKeybindings().getKeys(keybinding);
  const keyStr = keys.join("/");
  return t.fg("dim", keyStr) + t.fg("muted", ` ${description}`);
}

/**
 * Format a raw key string (not a keybinding id) with themed styling.
 */
export function rawKeyHint(key: string, description: string): string {
  const t = getTheme();
  return t.fg("dim", key) + t.fg("muted", ` ${description}`);
}

/**
 * Get dynamic keybinding hints for the current app context.
 */
export function getContextHints(context: AppContext): string {
  const t = getTheme();
  const hints = DEFAULT_HINTS.filter((h) => {
    if (Array.isArray(h.context)) return h.context.includes(context);
    return h.context === context;
  });

  if (hints.length === 0) return "";

  return hints
    .map((h) => `${t.fg("dim", h.key)} ${t.fg("muted", h.label)}`)
    .join(t.fg("dim", "  |  "));
}
