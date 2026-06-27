export interface TuiPreferencesData {
  defaultModel?: string;
  defaultProvider?: string;
  defaultThinkingLevel?: string;
  hideThinkingBlock?: boolean;
  theme?: string;
  sessionDir?: string;
  modelScopes?: string[];
}

type TuiPreferencesListener = (settings: TuiPreferencesData) => void;

/**
 * TUI-local display preferences and CLI overrides.
 *
 * Host-owned settings, auth, model discovery, tools, compaction, and retry policy
 * live behind hostd. This class only carries the small set of values the TUI
 * needs before the first hostd snapshot arrives.
 */
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

  getDefaultModel(): string | undefined {
    return this.store.defaultModel;
  }

  getDefaultProvider(): string | undefined {
    return this.store.defaultProvider;
  }

  getDefaultThinkingLevel(): string | undefined {
    return this.store.defaultThinkingLevel;
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

  getEnabledModels(): string[] | undefined {
    return this.store.modelScopes;
  }

  setDefaultModelAndProvider(provider: string, modelId: string): void {
    this.store.defaultProvider = provider;
    this.store.defaultModel = modelId;
    this.emit();
  }

  setDefaultThinkingLevel(level: string): void {
    this.store.defaultThinkingLevel = level;
    this.emit();
  }

  setTheme(theme: string): void {
    this.store.theme = theme;
    this.emit();
  }

  setHideThinkingBlock(hide: boolean): void {
    this.store.hideThinkingBlock = hide;
    this.emit();
  }

  applyOverrides(overrides: Partial<TuiPreferencesData>): void {
    Object.assign(this.store, overrides);
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
