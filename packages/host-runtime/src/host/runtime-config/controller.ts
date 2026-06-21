import type { HostConfig, ModelRegistry } from "../../models/index.js";
import type { SessionManager, SessionPersistenceOverview } from "../../session/index.js";
import type { HostState } from "../state/index.js";
import { restoreRuntimeFromSession } from "./restore.js";

export interface RuntimeConfigSnapshot {
  config: HostConfig;
  thinkingLevel: string;
  activeToolNames: string[] | undefined;
}

export class HostRuntimeConfigController {
  private config: HostConfig;
  private thinkingLevel = "off";
  private activeToolNames: string[] | undefined;

  constructor(
    initialConfig: HostConfig,
    private readonly getSessionManager: () => SessionManager,
    private readonly state: HostState,
    private readonly refreshPersistenceOverview: () => Promise<SessionPersistenceOverview>,
    private readonly modelRegistry?: ModelRegistry,
    initialThinkingLevel?: string,
  ) {
    this.config = initialConfig;
    if (initialThinkingLevel) {
      this.thinkingLevel = initialThinkingLevel;
      this.config = {
        ...this.config,
        settings: {
          ...this.config.settings,
          thinkingLevel: initialThinkingLevel !== "off" ? initialThinkingLevel : undefined,
        },
      };
    }
  }

  getConfig(): HostConfig {
    return this.config;
  }

  setConfig(config: HostConfig): void {
    const oldModel = this.config.model;
    this.config = {
      ...config,
      settings: {
        ...config.settings,
        thinkingLevel: this.thinkingLevel !== "off" ? this.thinkingLevel : undefined,
      },
    };
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
      // Propagate to config so orchestrator receives it
      this.config = {
        ...this.config,
        settings: {
          ...this.config.settings,
          thinkingLevel: level !== "off" ? level : undefined,
        },
      };
      this.getSessionManager()
        .appendThinkingLevelChange(level)
        .catch(() => {});
    }
  }

  getActiveToolNames(): string[] | undefined {
    return this.activeToolNames;
  }

  setActiveToolNames(toolNames: string[] | undefined): void {
    this.activeToolNames = toolNames;
    this.getSessionManager()
      .appendActiveToolsChange(this.getActiveToolNames() ?? [])
      .catch(() => {});
  }

  snapshot(): RuntimeConfigSnapshot {
    return {
      config: this.config,
      thinkingLevel: this.thinkingLevel,
      activeToolNames: this.activeToolNames,
    };
  }

  restoreSnapshot(snapshot: RuntimeConfigSnapshot): void {
    this.config = {
      ...snapshot.config,
      settings: {
        ...snapshot.config.settings,
        thinkingLevel: snapshot.thinkingLevel !== "off" ? snapshot.thinkingLevel : undefined,
      },
    };
    this.thinkingLevel = snapshot.thinkingLevel;
    this.activeToolNames = snapshot.activeToolNames;
  }

  applyTemporary(options: {
    config?: HostConfig;
    thinkingLevel?: string;
    activeToolNames?: string[];
  }): RuntimeConfigSnapshot {
    const snapshot = this.snapshot();
    if (options.config) {
      this.config = {
        ...options.config,
        settings: {
          ...options.config.settings,
          thinkingLevel: this.thinkingLevel !== "off" ? this.thinkingLevel : undefined,
        },
      };
    }
    if (options.thinkingLevel) {
      this.thinkingLevel = options.thinkingLevel;
      this.config = {
        ...this.config,
        settings: {
          ...this.config.settings,
          thinkingLevel: options.thinkingLevel !== "off" ? options.thinkingLevel : undefined,
        },
      };
    }
    if (options.activeToolNames !== undefined) {
      this.activeToolNames =
        options.activeToolNames.length > 0 ? [...options.activeToolNames] : undefined;
    }
    return snapshot;
  }

  async restoreFromSession(): Promise<void> {
    const result = await restoreRuntimeFromSession(
      this.getSessionManager(),
      this.config,
      this.modelRegistry,
    );
    if (result.config) {
      this.config = {
        ...result.config,
        settings: {
          ...result.config.settings,
          thinkingLevel: this.thinkingLevel !== "off" ? this.thinkingLevel : undefined,
        },
      };
    }
    if (result.thinkingLevel !== undefined) {
      this.thinkingLevel = result.thinkingLevel;
      this.config = {
        ...this.config,
        settings: {
          ...this.config.settings,
          thinkingLevel: result.thinkingLevel !== "off" ? result.thinkingLevel : undefined,
        },
      };
    }
    this.activeToolNames = result.hasActiveToolsEntry ? result.activeToolNames : undefined;
    if (result.sessionPersistenceOverview) {
      this.state.setSessionPersistenceOverview(result.sessionPersistenceOverview);
    } else {
      await this.refreshPersistenceOverview();
    }
  }
}
