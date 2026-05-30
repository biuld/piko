import { getKeybindings, type Keybinding } from "@earendil-works/pi-tui";
import { getTheme } from "../theme.js";

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
