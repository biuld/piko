import type { HostConfig, ModelRegistry } from "../../models/index.js";
import type { SessionManager, SessionPersistenceOverview } from "../../session/index.js";
import {
  type ActiveToolsState,
  activeToolNamesFromState,
  activeToolsStateFromNames,
} from "../../turn-state.js";
import type { HostState } from "../state/index.js";
import { restoreRuntimeFromSession } from "./restore.js";

export interface RuntimeConfigSnapshot {
  config: HostConfig;
  thinkingLevel: string;
  activeToolsState: ActiveToolsState;
}

export class HostRuntimeConfigController {
  private config: HostConfig;
  private thinkingLevel = "off";
  private activeToolsState: ActiveToolsState = { kind: "all" };

  constructor(
    initialConfig: HostConfig,
    private readonly getSessionManager: () => SessionManager,
    private readonly state: HostState,
    private readonly refreshPersistenceOverview: () => Promise<SessionPersistenceOverview>,
    private readonly modelRegistry?: ModelRegistry,
  ) {
    this.config = initialConfig;
  }

  getConfig(): HostConfig {
    return this.config;
  }

  setConfig(config: HostConfig): void {
    const oldModel = this.config.model;
    if (!config.tools?.length && this.config.tools?.length) {
      config = { ...config, tools: this.config.tools };
    }
    this.config = config;
    if (config.model.provider !== oldModel.provider || config.model.id !== oldModel.id) {
      this.getSessionManager()
        .appendModelChange(config.model.provider, config.model.id)
        .catch(() => {});
    }
  }

  getThinkingLevel(): string {
    return this.thinkingLevel;
  }

  setThinkingLevel(level: string): void {
    if (this.thinkingLevel !== level) {
      this.thinkingLevel = level;
      this.getSessionManager()
        .appendThinkingLevelChange(level)
        .catch(() => {});
    }
  }

  getActiveToolNames(): string[] | undefined {
    return activeToolNamesFromState(this.activeToolsState);
  }

  setActiveToolNames(toolNames: string[] | undefined): void {
    this.activeToolsState = activeToolsStateFromNames(toolNames);
    this.getSessionManager()
      .appendActiveToolsChange(this.getActiveToolNames() ?? [])
      .catch(() => {});
  }

  snapshot(): RuntimeConfigSnapshot {
    return {
      config: this.config,
      thinkingLevel: this.thinkingLevel,
      activeToolsState: this.activeToolsState,
    };
  }

  restoreSnapshot(snapshot: RuntimeConfigSnapshot): void {
    this.config = snapshot.config;
    this.thinkingLevel = snapshot.thinkingLevel;
    this.activeToolsState = snapshot.activeToolsState;
  }

  applyTemporary(options: {
    config?: HostConfig;
    thinkingLevel?: string;
    activeToolNames?: string[];
  }): RuntimeConfigSnapshot {
    const snapshot = this.snapshot();
    if (options.config) this.config = options.config;
    if (options.thinkingLevel) this.thinkingLevel = options.thinkingLevel;
    if (options.activeToolNames !== undefined) {
      this.activeToolsState = activeToolsStateFromNames(options.activeToolNames);
    }
    return snapshot;
  }

  async restoreFromSession(): Promise<void> {
    const result = await restoreRuntimeFromSession(
      this.getSessionManager(),
      this.config,
      this.modelRegistry,
    );
    if (result.config) this.config = result.config;
    if (result.thinkingLevel !== undefined) this.thinkingLevel = result.thinkingLevel;
    this.activeToolsState = activeToolsStateFromNames(
      result.hasActiveToolsEntry ? result.activeToolNames : undefined,
    );
    if (result.sessionPersistenceOverview) {
      this.state.setSessionPersistenceOverview(result.sessionPersistenceOverview);
    } else {
      await this.refreshPersistenceOverview();
    }
  }
}
