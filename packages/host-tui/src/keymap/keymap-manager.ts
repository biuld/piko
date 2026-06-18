// ============================================================================
// KeymapManager — key matching, display formatting, conflict detection
// ============================================================================

import { joinPath, resolvePath } from "piko-host-runtime";
import { DEFAULT_KEYBINDINGS, formatKeyCombo, KEYBINDING_LABELS } from "./defaults.js";
import {
  type KeybindingEntry,
  type KeybindingId,
  type KeybindingScope,
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
   * Load keybinding overrides from global and project config files.
   * Resolution order: global (~/.piko/keybindings.json) then project (.piko/keybindings.json).
   * Returns an array of conflicts found after loading.
   */
  async loadFromFiles(
    cwd: string,
  ): Promise<Array<{ id1: KeybindingId; id2: KeybindingId; key: string }>> {
    const pikoDir =
      process.env.PIKO_DIR ??
      joinPath(process.env.HOME ?? process.env.USERPROFILE ?? "/tmp", ".piko");
    const globalPath = joinPath(pikoDir, "keybindings.json");
    const projectPath = joinPath(resolvePath(cwd), ".piko", "keybindings.json");

    // Load global first, then project (project overrides global)
    const globalOverrides = await this.loadOverrideFile(globalPath);
    if (globalOverrides.length > 0) {
      this.applyOverrides(globalOverrides);
    }

    const projectOverrides = await this.loadOverrideFile(projectPath);
    if (projectOverrides.length > 0) {
      this.applyOverrides(projectOverrides);
    }

    return this.detectConflicts();
  }

  /**
   * Load override entries from a JSON file.
   * File format: { "bindings": { "app.exit": "ctrl+q", ... } }
   */
  private async loadOverrideFile(filePath: string): Promise<KeymapOverride[]> {
    try {
      if (!(await Bun.file(filePath).exists())) return [];
      const content = await Bun.file(filePath).text();
      const data = JSON.parse(content);
      const bindings = data?.bindings ?? data;
      if (typeof bindings !== "object" || bindings === null) return [];

      return Object.entries(bindings).map(([id, keys]) => ({
        id,
        keys: String(keys),
      }));
    } catch {
      return [];
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
   * Detect conflicting keybindings (same key combo, different binding IDs).
   * When `scope` is provided, only considers bindings with that scope.
   * Returns pairs of conflicting IDs.
   */
  detectConflicts(
    scope?: KeybindingScope,
  ): Array<{ id1: KeybindingId; id2: KeybindingId; key: string }> {
    const conflicts: Array<{ id1: KeybindingId; id2: KeybindingId; key: string }> = [];
    const byCombo = new Map<string, KeybindingId[]>();

    for (const [id, entry] of this.bindings) {
      // Scope filter: only check bindings within the same scope
      if (scope && (entry.scope ?? "global") !== scope) continue;
      const keys = this.overrides.get(id) ?? entry.keys;
      if (!keys.key) continue; // Skip placeholder entries
      const comboStr = formatKeyCombo(keys);
      const list = byCombo.get(comboStr) ?? [];
      list.push(id);
      byCombo.set(comboStr, list);
    }

    for (const [comboStr, ids] of byCombo) {
      for (let i = 0; i < ids.length; i++) {
        for (let j = i + 1; j < ids.length; j++) {
          conflicts.push({ id1: ids[i], id2: ids[j], key: comboStr });
        }
      }
    }

    return conflicts;
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
   * Format a hint line from an array of (bindingId, description) tuples.
   * Example: `[['tui.select.up', 'navigate'], ['tui.select.cancel', 'close']]`
   * produces `↑↓ navigate  Esc close`
   */
  formatHintLine(hints: Array<[KeybindingId, string]>): string {
    return hints
      .map(([id, desc]) => {
        const display = this.keyDisplayText(id);
        return desc ? `${display} ${desc}` : display;
      })
      .join("  ");
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
