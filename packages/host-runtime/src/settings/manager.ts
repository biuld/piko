/**
 * Settings Manager — layered configuration management.
 *
 * Layers (highest to lowest precedence):
 * 1. CLI flags / runtime overrides
 * 2. Project settings (.piko/settings.json)
 * 3. Global settings (~/.piko/settings.json)
 * 4. Hardcoded defaults
 */

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import type { Transport } from "@earendil-works/pi-ai";
import { getPikoDir } from "../session/index.js";

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
  sessionDir?: string;
  extensions?: string[];
  skills?: string[];
  prompts?: string[];
  themes?: string[];
  enabledModels?: string[];
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

function loadFromFile(filePath: string): Settings {
  if (!existsSync(filePath)) return {};

  try {
    const content = readFileSync(filePath, "utf-8");
    return JSON.parse(content) as Settings;
  } catch {
    return {};
  }
}

function saveToFile(filePath: string, settings: Settings): void {
  const dir = dirname(filePath);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }
  writeFileSync(filePath, JSON.stringify(settings, null, 2), "utf-8");
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
  static create(cwd: string): SettingsManager {
    const resolvedCwd = resolve(cwd);
    const globalPath = join(getPikoDir(), "settings.json");
    const projectPath = join(resolvedCwd, ".piko", "settings.json");

    const globalSettings = loadFromFile(globalPath);
    const projectSettings = loadFromFile(projectPath);

    return new SettingsManager(globalPath, projectPath, globalSettings, projectSettings);
  }

  /** Create an in-memory SettingsManager (for tests). */
  static inMemory(settings: Partial<Settings> = {}): SettingsManager {
    return new SettingsManager("", "", {}, {}, settings);
  }

  // ---- Reload ----

  reload(): void {
    this.globalSettings = loadFromFile(this.globalPath);
    this.projectSettings = loadFromFile(this.projectPath);

    this.mergedSettings = deepMerge(deepMerge(DEFAULTS, this.globalSettings), this.projectSettings);
    this.mergedSettings = deepMerge(this.mergedSettings, this.overrides);
  }

  // ---- Apply overrides ----

  applyOverrides(overrides: Partial<Settings>): void {
    this.overrides = deepMerge(this.overrides, overrides);
    this.mergedSettings = deepMerge(this.mergedSettings, overrides);
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

  // ---- Mutators (persist to global settings) ----

  private persistGlobal(): void {
    if (!this.globalPath) return;
    saveToFile(this.globalPath, this.globalSettings);
  }

  private persistProject(): void {
    if (!this.projectPath) return;
    saveToFile(this.projectPath, this.projectSettings);
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

  setRetryEnabled(enabled: boolean): void {
    if (!this.globalSettings.retry) this.globalSettings.retry = {};
    this.globalSettings.retry.enabled = enabled;
    if (!this.mergedSettings.retry) this.mergedSettings.retry = {};
    this.mergedSettings.retry.enabled = enabled;
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
}
