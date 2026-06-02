// ============================================================================
// KeymapManager — key matching, display formatting, conflict detection
// ============================================================================

import { DEFAULT_KEYBINDINGS, formatKeyCombo, KEYBINDING_LABELS } from "./defaults.js";
import {
  type KeybindingEntry,
  type KeybindingId,
  type KeyCombo,
  keyComboMatches,
} from "./types.js";

export interface KeymapOverride {
  id: string;
  keys: string; // serialized key combo
}

export class KeymapManager {
  private bindings: Map<KeybindingId, KeybindingEntry> = new Map();
  private overrides: Map<KeybindingId, KeyCombo> = new Map();

  constructor() {
    // Load defaults
    for (const entry of DEFAULT_KEYBINDINGS) {
      this.bindings.set(entry.id, { ...entry });
    }
  }

  /**
   * Apply user/project overrides from config.
   */
  applyOverrides(overrides: KeymapOverride[]): void {
    for (const override of overrides) {
      const id = override.id as KeybindingId;
      const entry = this.bindings.get(id);
      if (!entry) continue;

      const combo = this.parseKeyString(override.keys);
      if (!combo) continue;

      this.overrides.set(id, combo);
    }
  }

  /**
   * Get the effective key combo for a binding ID (override or default).
   */
  getKeys(id: KeybindingId): KeyCombo | undefined {
    return this.overrides.get(id) ?? this.bindings.get(id)?.keys;
  }

  /**
   * Find a matching keybinding for a key event.
   */
  findBinding(
    keyName: string,
    ctrl: boolean,
    shift: boolean,
    alt: boolean,
    meta: boolean,
  ): KeybindingId | undefined {
    for (const [id, entry] of this.bindings) {
      const keys = this.overrides.get(id) ?? entry.keys;
      if (keyComboMatches(keys, keyName, ctrl, shift, alt, meta)) {
        return id;
      }
    }
    return undefined;
  }

  /**
   * Check if a binding requires the stream to be idle.
   */
  requiresIdle(id: KeybindingId): boolean {
    return this.bindings.get(id)?.requiresIdle ?? false;
  }

  /**
   * Get the human-readable display text for a keybinding.
   */
  keyText(id: KeybindingId): string {
    return KEYBINDING_LABELS[id] ?? id;
  }

  /**
   * Get the formatted key combination display text.
   */
  keyDisplayText(id: KeybindingId): string {
    const keys = this.getKeys(id);
    if (!keys) return id;
    return formatKeyCombo(keys);
  }

  /**
   * Get a hint string for a keybinding + description.
   */
  keyHint(id: KeybindingId, description: string): string {
    const display = this.keyDisplayText(id);
    if (!display) return description;
    return `${display} ${description}`;
  }

  /**
   * Get a raw key hint (not backed by a registered binding).
   */
  rawKeyHint(key: string, description: string): string {
    return `${key} ${description}`;
  }

  /**
   * Get all bindings for display.
   */
  listBindings(): Array<{ id: KeybindingId; keys: KeyCombo; requiresIdle: boolean }> {
    return [...this.bindings.entries()].map(([id, entry]) => ({
      id,
      keys: this.overrides.get(id) ?? entry.keys,
      requiresIdle: entry.requiresIdle ?? false,
    }));
  }

  /**
   * Parse a key string like "ctrl+shift+p" into a KeyCombo.
   */
  private parseKeyString(s: string): KeyCombo | null {
    if (!s) return null;
    const lower = s.toLowerCase().trim();
    const parts = lower.split(/[+-]/).map((p) => p.trim());

    let ctrl = false;
    let shift = false;
    let alt = false;
    let meta = false;
    let key = "";

    for (const part of parts) {
      switch (part) {
        case "ctrl":
        case "control":
          ctrl = true;
          break;
        case "shift":
          shift = true;
          break;
        case "alt":
        case "option":
          alt = true;
          break;
        case "meta":
        case "cmd":
        case "command":
        case "super":
          meta = true;
          break;
        default:
          key = part;
          break;
      }
    }

    if (!key) return null;

    return { key, ctrl, shift, alt, meta };
  }
}
