import type { TuiHostFacade } from "../../app/tui-host.js";
import {
  createHostConfig,
  type Model,
  type ModelProviderConfig,
  type SettingsManager,
} from "../../shared/index.js";
import type { HostdActionAdapter } from "./hostd-action-adapter.js";

export class RuntimeConfigAdapter {
  constructor(
    private readonly host: TuiHostFacade,
    private readonly hostd: HostdActionAdapter,
    private readonly settingsManager: SettingsManager,
  ) {}

  applyModel(model: Model<string>, providerConfig: ModelProviderConfig): void {
    const currentConfig = this.host.getConfig();
    this.host.setConfig(createHostConfig(model, providerConfig, currentConfig.settings));
    this.hostd.setModel(model.provider, model.id);
    this.settingsManager.setDefaultModelAndProvider(model.provider, model.id);
  }

  applyThinkingLevel(level: string): void {
    this.host.setThinkingLevel(level);
    this.hostd.setThinkingLevel(level);
    this.settingsManager.setDefaultThinkingLevel(level as any);
  }
}
