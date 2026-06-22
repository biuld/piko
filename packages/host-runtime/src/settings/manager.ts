/**
 * Settings Manager — layered configuration management.
 *
 * Layers (highest to lowest precedence):
 * 1. CLI flags / runtime overrides
 * 2. Project settings (.piko/settings.json)
 * 3. Global settings (~/.piko/settings.json)
 * 4. Hardcoded defaults
 */

import type { Transport } from "@earendil-works/pi-ai";
import { getPikoDir } from "../session/index.js";
import { joinPath, resolvePath } from "../utils/bun-path.js";

export type TransportSetting = Transport;

// ============================================================================
// Settings types
// ============================================================================

export interface CompactionSettings {
  enabled?: boolean;
  reserveTokens?: number;
  keepRecentTokens?: number;
}

export interface BranchSummarySettings {
  reserveTokens?: number;
  skipPrompt?: boolean;
}

export interface RetrySettings {
  enabled?: boolean;
  maxRetries?: number;
  baseDelayMs?: number;
}

export interface McpServerConfig {
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

export interface SandboxSettings {
  /** Enable the external piko-sandbox supervisor. Disabled by default. */
  enabled?: boolean;
  /** Policy file. Relative paths are resolved from the session workspace. */
  policyPath?: string;
  /** piko-sandbox executable name or absolute path. */
  binaryPath?: string;
}

export interface Settings {
  defaultProvider?: string;
  defaultModel?: string;
  defaultThinkingLevel?: "off" | "minimal" | "low" | "medium" | "high" | "xhigh";
  transport?: TransportSetting;
  theme?: string;
  compaction?: CompactionSettings;
  branchSummary?: BranchSummarySettings;
  retry?: RetrySettings;
  hideThinkingBlock?: boolean;
  shellPath?: string;
  sandbox?: SandboxSettings;
  sessionDir?: string;
  /** Action for double-escape with empty editor: show tree, fork panel, or nothing (default: "tree"). */
  doubleEscapeAction?: "fork" | "tree" | "none";
  /** Suppress startup messages (default: false). */
  quietStartup?: boolean;
  /** Clear screen on terminal shrink to avoid garbled output (default: true). */
  clearOnShrink?: boolean;
  /** Steering mode: consume all at once or one at a time (default: "all"). */
  steeringMode?: "all" | "one-at-a-time";
  /** Follow-up mode: consume all at once or one at a time (default: "one-at-a-time"). */
  followUpMode?: "all" | "one-at-a-time";
  extensions?: string[];
  skills?: string[];
  prompts?: string[];
  themes?: string[];
  enabledModels?: string[];
  mcpServers?: Record<string, McpServerConfig>;
  /** Agent display name pool. Each spawned agent picks the next name in round-robin order. */
  agentNames?: string[];
}

// ============================================================================
// Defaults
// ============================================================================

const DEFAULTS: Settings = {
  compaction: {
    enabled: true,
    reserveTokens: 16384,
    keepRecentTokens: 20000,
  },
  branchSummary: {
    reserveTokens: 16384,
    skipPrompt: false,
  },
  retry: {
    enabled: true,
    maxRetries: 3,
    baseDelayMs: 2000,
  },
};

// ============================================================================
// Deep merge
// ============================================================================

function deepMerge(base: Settings, overrides: Settings): Settings {
  const result: Settings = { ...base };

  for (const key of Object.keys(overrides) as (keyof Settings)[]) {
    const overrideValue = overrides[key];
    const baseValue = base[key];

    if (overrideValue === undefined) continue;

    if (
      typeof overrideValue === "object" &&
      overrideValue !== null &&
      !Array.isArray(overrideValue) &&
      typeof baseValue === "object" &&
      baseValue !== null &&
      !Array.isArray(baseValue)
    ) {
      (result as Record<string, unknown>)[key] = { ...baseValue, ...overrideValue };
    } else {
      (result as Record<string, unknown>)[key] = overrideValue;
    }
  }

  return result;
}

// ============================================================================
// Loader
// ============================================================================

async function loadFromFile(filePath: string): Promise<Settings> {
  if (!filePath || !(await Bun.file(filePath).exists())) return {};
  try {
    const content = await Bun.file(filePath).text();
    return JSON.parse(content) as Settings;
  } catch {
    return {};
  }
}

async function saveToFile(filePath: string, settings: Settings): Promise<void> {
  if (!filePath) return;
  await Bun.write(filePath, JSON.stringify(settings, null, 2), { createPath: true });
}

// ============================================================================
// Manager
// ============================================================================

export class SettingsManager {
  private globalPath: string;
  private projectPath: string;
  private globalSettings: Settings;
  private projectSettings: Settings;
  private mergedSettings: Settings;
  private overrides: Settings;
  private pendingPersist: Promise<void> = Promise.resolve();
  private listeners: Set<(settings: Settings) => void> = new Set();

  private constructor(
    globalPath: string,
    projectPath: string,
    globalSettings: Settings,
    projectSettings: Settings,
    overrides: Settings = {},
  ) {
    this.globalPath = globalPath;
    this.projectPath = projectPath;
    this.globalSettings = globalSettings;
    this.projectSettings = projectSettings;
    this.overrides = overrides;
    this.mergedSettings = deepMerge(deepMerge(DEFAULTS, globalSettings), projectSettings);
    this.mergedSettings = deepMerge(this.mergedSettings, overrides);
  }

  /** Create a SettingsManager from files on disk. */
  static async create(cwd: string): Promise<SettingsManager> {
    const resolvedCwd = resolvePath(cwd);
    const globalPath = joinPath(getPikoDir(), "settings.json");
    const projectPath = joinPath(resolvedCwd, ".piko", "settings.json");

    const globalSettings = await loadFromFile(globalPath);
    const projectSettings = await loadFromFile(projectPath);

    return new SettingsManager(globalPath, projectPath, globalSettings, projectSettings);
  }

  /** Create an in-memory SettingsManager (for tests). */
  static inMemory(settings: Partial<Settings> = {}): SettingsManager {
    return new SettingsManager("", "", {}, {}, settings);
  }

  // ---- Reload ----

  async reload(): Promise<void> {
    this.globalSettings = await loadFromFile(this.globalPath);
    this.projectSettings = await loadFromFile(this.projectPath);

    this.mergedSettings = deepMerge(deepMerge(DEFAULTS, this.globalSettings), this.projectSettings);
    this.mergedSettings = deepMerge(this.mergedSettings, this.overrides);
  }

  // ---- Apply overrides ----

  applyOverrides(overrides: Partial<Settings>): void {
    this.overrides = deepMerge(this.overrides, overrides);
    this.mergedSettings = deepMerge(this.mergedSettings, overrides);
    this.notifyListeners();
  }

  // ---- Accessors ----

  get settings(): Settings {
    return { ...this.mergedSettings };
  }

  getGlobalSettings(): Settings {
    return { ...this.globalSettings };
  }

  getProjectSettings(): Settings {
    return { ...this.projectSettings };
  }

  getDefaultProvider(): string | undefined {
    return this.mergedSettings.defaultProvider;
  }

  getDefaultModel(): string | undefined {
    return this.mergedSettings.defaultModel;
  }

  getDefaultThinkingLevel(): "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | undefined {
    return this.mergedSettings.defaultThinkingLevel;
  }

  getTransport(): TransportSetting {
    return this.mergedSettings.transport ?? "auto";
  }

  getTheme(): string | undefined {
    return this.mergedSettings.theme;
  }

  getCompactionSettings(): { enabled: boolean; reserveTokens: number; keepRecentTokens: number } {
    return {
      enabled: this.mergedSettings.compaction?.enabled ?? true,
      reserveTokens: this.mergedSettings.compaction?.reserveTokens ?? 16384,
      keepRecentTokens: this.mergedSettings.compaction?.keepRecentTokens ?? 20000,
    };
  }

  getBranchSummarySettings(): { reserveTokens: number; skipPrompt: boolean } {
    return {
      reserveTokens: this.mergedSettings.branchSummary?.reserveTokens ?? 16384,
      skipPrompt: this.mergedSettings.branchSummary?.skipPrompt ?? false,
    };
  }

  getRetrySettings(): { enabled: boolean; maxRetries: number; baseDelayMs: number } {
    return {
      enabled: this.mergedSettings.retry?.enabled ?? true,
      maxRetries: this.mergedSettings.retry?.maxRetries ?? 3,
      baseDelayMs: this.mergedSettings.retry?.baseDelayMs ?? 2000,
    };
  }

  getHideThinkingBlock(): boolean {
    return this.mergedSettings.hideThinkingBlock ?? false;
  }

  getSessionDir(): string | undefined {
    return this.mergedSettings.sessionDir;
  }

  getExtensionPaths(): string[] {
    return this.mergedSettings.extensions ?? [];
  }

  getSkillPaths(): string[] {
    return this.mergedSettings.skills ?? [];
  }

  getPromptTemplatePaths(): string[] {
    return this.mergedSettings.prompts ?? [];
  }

  getThemePaths(): string[] {
    return this.mergedSettings.themes ?? [];
  }

  getEnabledModels(): string[] | undefined {
    return this.mergedSettings.enabledModels;
  }

  getAgentNames(): string[] {
    return this.mergedSettings.agentNames ?? [];
  }

  // ---- Mutators (persist to global settings) ----

  onChange(listener: (settings: Settings) => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private notifyListeners(): void {
    const s = this.settings;
    for (const listener of this.listeners) {
      try {
        listener(s);
      } catch (err) {
        console.error("Error in settings listener:", err);
      }
    }
  }

  private persistGlobal(): void {
    this.notifyListeners();
    if (!this.globalPath) return;
    this.pendingPersist = this.pendingPersist
      .catch(() => {})
      .then(() => saveToFile(this.globalPath, this.globalSettings));
  }

  async flush(): Promise<void> {
    await this.pendingPersist;
  }

  setDefaultModel(modelId: string): void {
    this.globalSettings.defaultModel = modelId;
    this.mergedSettings.defaultModel = modelId;
    this.persistGlobal();
  }

  setDefaultProvider(provider: string): void {
    this.globalSettings.defaultProvider = provider;
    this.mergedSettings.defaultProvider = provider;
    this.persistGlobal();
  }

  setDefaultModelAndProvider(provider: string, modelId: string): void {
    this.globalSettings.defaultProvider = provider;
    this.globalSettings.defaultModel = modelId;
    this.mergedSettings.defaultProvider = provider;
    this.mergedSettings.defaultModel = modelId;
    this.persistGlobal();
  }

  setDefaultThinkingLevel(level: "off" | "minimal" | "low" | "medium" | "high" | "xhigh"): void {
    this.globalSettings.defaultThinkingLevel = level;
    this.mergedSettings.defaultThinkingLevel = level;
    this.persistGlobal();
  }

  setTransport(transport: TransportSetting): void {
    this.globalSettings.transport = transport;
    this.mergedSettings.transport = transport;
    this.persistGlobal();
  }

  setTheme(theme: string): void {
    this.globalSettings.theme = theme;
    this.mergedSettings.theme = theme;
    this.persistGlobal();
  }

  setCompactionEnabled(enabled: boolean): void {
    if (!this.globalSettings.compaction) this.globalSettings.compaction = {};
    this.globalSettings.compaction.enabled = enabled;
    if (!this.mergedSettings.compaction) this.mergedSettings.compaction = {};
    this.mergedSettings.compaction.enabled = enabled;
    this.persistGlobal();
  }

  setCompactionReserveTokens(tokens: number): void {
    if (!this.globalSettings.compaction) this.globalSettings.compaction = {};
    this.globalSettings.compaction.reserveTokens = tokens;
    if (!this.mergedSettings.compaction) this.mergedSettings.compaction = {};
    this.mergedSettings.compaction.reserveTokens = tokens;
    this.persistGlobal();
  }

  setCompactionKeepRecentTokens(tokens: number): void {
    if (!this.globalSettings.compaction) this.globalSettings.compaction = {};
    this.globalSettings.compaction.keepRecentTokens = tokens;
    if (!this.mergedSettings.compaction) this.mergedSettings.compaction = {};
    this.mergedSettings.compaction.keepRecentTokens = tokens;
    this.persistGlobal();
  }

  setRetryEnabled(enabled: boolean): void {
    if (!this.globalSettings.retry) this.globalSettings.retry = {};
    this.globalSettings.retry.enabled = enabled;
    if (!this.mergedSettings.retry) this.mergedSettings.retry = {};
    this.mergedSettings.retry.enabled = enabled;
    this.persistGlobal();
  }

  setRetryMaxRetries(maxRetries: number): void {
    if (!this.globalSettings.retry) this.globalSettings.retry = {};
    this.globalSettings.retry.maxRetries = maxRetries;
    if (!this.mergedSettings.retry) this.mergedSettings.retry = {};
    this.mergedSettings.retry.maxRetries = maxRetries;
    this.persistGlobal();
  }

  setHideThinkingBlock(hide: boolean): void {
    this.globalSettings.hideThinkingBlock = hide;
    this.mergedSettings.hideThinkingBlock = hide;
    this.persistGlobal();
  }

  setEnabledModels(patterns: string[] | undefined): void {
    this.globalSettings.enabledModels = patterns;
    this.mergedSettings.enabledModels = patterns;
    this.persistGlobal();
  }

  getDoubleEscapeAction(): "fork" | "tree" | "none" {
    return this.mergedSettings.doubleEscapeAction ?? "tree";
  }

  setDoubleEscapeAction(action: "fork" | "tree" | "none"): void {
    this.globalSettings.doubleEscapeAction = action;
    this.mergedSettings.doubleEscapeAction = action;
    this.persistGlobal();
  }

  getShellPath(): string | undefined {
    return this.mergedSettings.shellPath;
  }

  getSandboxSettings(): { enabled: boolean; policyPath: string; binaryPath: string } {
    return {
      enabled: this.mergedSettings.sandbox?.enabled ?? false,
      policyPath: this.mergedSettings.sandbox?.policyPath ?? ".piko/sandbox.json",
      binaryPath: this.mergedSettings.sandbox?.binaryPath ?? "piko-sandbox",
    };
  }

  setShellPath(path: string): void {
    this.globalSettings.shellPath = path;
    this.mergedSettings.shellPath = path;
    this.persistGlobal();
  }

  setSessionDir(dir: string): void {
    this.globalSettings.sessionDir = dir;
    this.mergedSettings.sessionDir = dir;
    this.persistGlobal();
  }

  getQuietStartup(): boolean {
    return this.mergedSettings.quietStartup ?? false;
  }

  setQuietStartup(quiet: boolean): void {
    this.globalSettings.quietStartup = quiet;
    this.mergedSettings.quietStartup = quiet;
    this.persistGlobal();
  }

  getClearOnShrink(): boolean {
    return this.mergedSettings.clearOnShrink ?? true;
  }

  setClearOnShrink(clear: boolean): void {
    this.globalSettings.clearOnShrink = clear;
    this.mergedSettings.clearOnShrink = clear;
    this.persistGlobal();
  }

  getSteeringMode(): "all" | "one-at-a-time" {
    return this.mergedSettings.steeringMode ?? "all";
  }

  setSteeringMode(mode: "all" | "one-at-a-time"): void {
    this.globalSettings.steeringMode = mode;
    this.mergedSettings.steeringMode = mode;
    this.persistGlobal();
  }

  getFollowUpMode(): "all" | "one-at-a-time" {
    return this.mergedSettings.followUpMode ?? "one-at-a-time";
  }

  setFollowUpMode(mode: "all" | "one-at-a-time"): void {
    this.globalSettings.followUpMode = mode;
    this.mergedSettings.followUpMode = mode;
    this.persistGlobal();
  }
}
