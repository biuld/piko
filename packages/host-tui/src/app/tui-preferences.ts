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

  static async create(_cwd: string): Promise<TuiPreferences> {
    return new TuiPreferences();
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
