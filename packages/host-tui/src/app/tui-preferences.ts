/**
 * TUI-local display preferences only.
 *
 * Host-owned model config, thinking level, and auth live behind hostd.
 * This class only carries the small set of values the TUI needs for
 * its own rendering before hostd responds.
 */
export interface TuiPreferencesData {
  theme?: string;
  hideThinkingBlock?: boolean;
  sessionDir?: string;
}

type TuiPreferencesListener = (settings: TuiPreferencesData) => void;

export class TuiPreferences {
  private listeners: TuiPreferencesListener[] = [];
  private store: TuiPreferencesData;

  get settings(): TuiPreferencesData {
    return { ...this.store };
  }

  constructor(store: TuiPreferencesData = {}) {
    this.store = { ...store };
  }

  static async create(cwd: string): Promise<TuiPreferences> {
    return TuiPreferences.loadFromHostSettings(cwd);
  }

  /** Read theme/hideThinkingBlock from hostd's settings.toml files. */
  static loadFromHostSettings(cwd: string): TuiPreferences {
    const store: TuiPreferencesData = {};
    try {
      const { readFileSync, existsSync } = require("node:fs");
      const { join } = require("node:path");
      const home = process.env.HOME || process.env.USERPROFILE || ".";

      for (const settingsPath of [join(home, ".piko", "settings.toml"), join(cwd, ".piko", "settings.toml")]) {
        if (!existsSync(settingsPath)) continue;
        try {
          const raw = readFileSync(settingsPath, "utf-8");
          // TOML: parse kebab-case keys. For the subset we care about, simple line-based extraction is sufficient.
          for (const line of raw.split("\n")) {
            const trimmed = line.trim();
            if (trimmed.startsWith("#") || trimmed === "") continue;
            const themeMatch = trimmed.match(/^theme\s*=\s*"([^"]+)"/);
            if (themeMatch) { store.theme = themeMatch[1]; continue; }
            const hideMatch = trimmed.match(/^hide-thinking-block\s*=\s*(true|false)/);
            if (hideMatch) { store.hideThinkingBlock = hideMatch[1] === "true"; continue; }
          }
        } catch { /* skip malformed */ }
      }
    } catch { /* fs not available */ }
    return new TuiPreferences(store);
  }

  static inMemory(settings: Partial<TuiPreferencesData> = {}): TuiPreferences {
    return new TuiPreferences(settings);
  }

  getTheme(): string | undefined {
    return this.store.theme;
  }

  getHideThinkingBlock(): boolean {
    return this.store.hideThinkingBlock ?? false;
  }

  getSessionDir(): string | undefined {
    return this.store.sessionDir;
  }

  setTheme(theme: string): void {
    this.store.theme = theme;
    this.emit();
  }

  setHideThinkingBlock(hide: boolean): void {
    this.store.hideThinkingBlock = hide;
    this.emit();
  }

  onChange(listener: TuiPreferencesListener): void {
    this.listeners.push(listener);
  }

  private emit(): void {
    const snapshot = this.settings;
    for (const listener of this.listeners) {
      listener(snapshot);
    }
  }
}
