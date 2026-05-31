import type { ModelRegistry, SettingsManager } from "piko-host-runtime";

export interface RunTuiOptions {
  session?: string;
  extensions?: import("../extensions/index.js").PikoExtensionFactory[];
  settingsManager?: SettingsManager;
  modelRegistry?: ModelRegistry;
  authStorage?: import("piko-host-runtime").AuthStorage;
  sessionName?: string;
  noContextFiles?: boolean;
  noTools?: boolean;
  systemPrompt?: string;
  appendSystemPrompt?: string;
}
