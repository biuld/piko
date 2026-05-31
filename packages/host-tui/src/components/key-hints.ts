import { getKeybindings, type Keybinding } from "@earendil-works/pi-tui";
import { getTheme } from "../theme.js";

/**
 * Context-aware keybinding hints.
 *
 * Dynamically shows available shortcuts based on current app state:
 * - normal: editing prompt
 * - streaming: abort available
 * - overlay open: overlay-specific keys
 */

export type AppContext = "normal" | "streaming" | "overlay" | "tree";

export interface KeyHintConfig {
  key: string;
  label: string;
  context: AppContext | AppContext[];
}

const DEFAULT_HINTS: KeyHintConfig[] = [
  { key: "Ctrl+D", label: "submit", context: "normal" },
  { key: "Ctrl+C", label: "exit", context: "normal" },
  { key: "Ctrl+P", label: "prev model", context: "normal" },
  { key: "Ctrl+N", label: "next model", context: "normal" },
  { key: "Ctrl+T", label: "theme", context: "normal" },
  { key: "Ctrl+F", label: "fork", context: "tree" },
  { key: "Ctrl+R", label: "rename", context: "tree" },
  { key: "Ctrl+C", label: "abort", context: "streaming" },
];

/**
 * Format a keybinding as human-readable text with themed dim/muted styling.
 * e.g., "Esc cancel" or "↑↓ navigate"
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
