import { createHostConfig } from "../../app/host-config.js";
import type { TuiHostFacade } from "../../app/tui-host.js";
import type { Model, ModelProviderConfig } from "../../shared/index.js";
import type { HostdActionAdapter } from "./hostd-action-adapter.js";

/**
 * Bridges model and thinking-level changes between the TUI facade and hostd.
 * No longer writes to TuiPreferences — model config is owned by hostd.
 */
export class RuntimeConfigAdapter {
  constructor(
    private readonly host: TuiHostFacade,
    private readonly hostd: HostdActionAdapter,
  ) {}

  applyModel(model: Model<string>, providerConfig: ModelProviderConfig): void {
    const currentConfig = this.host.getConfig();
    this.host.setConfig(createHostConfig(model, providerConfig, currentConfig.settings));
    this.hostd.setModel(model.provider, model.id);
  }

  applyThinkingLevel(level: string): void {
    this.host.setThinkingLevel(level);
    this.hostd.setThinkingLevel(level);
  }
}
