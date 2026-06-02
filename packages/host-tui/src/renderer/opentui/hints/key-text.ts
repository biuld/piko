// ============================================================================
// Key text helpers — format keybinding labels from KeymapManager
// ============================================================================

import type { KeymapManager } from "../../../keymap/keymap-manager.js";
import type { KeybindingId } from "../../../keymap/types.js";

/**
 * Get raw key text for a binding ID.
 */
export function keyText(keymap: KeymapManager, id: KeybindingId): string {
  return keymap.keyText(id);
}

/**
 * Get formatted key display text.
 */
export function keyDisplayText(keymap: KeymapManager, id: KeybindingId): string {
  return keymap.keyDisplayText(id);
}

/**
 * Build a hint string from a keybinding ID and description.
 */
export function keyHint(keymap: KeymapManager, id: KeybindingId, description: string): string {
  return keymap.keyHint(id, description);
}

/**
 * Build a raw hint string from a key and description.
 */
export function rawKeyHint(key: string, description: string): string {
  return `${key} ${description}`;
}
