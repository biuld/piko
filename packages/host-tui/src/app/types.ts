export interface RunTuiOptions {
  session?: string;
  extensions?: import("../extensions/index.js").PikoExtensionFactory[];
  settingsManager?: import("piko-host-runtime").SettingsManager;
  modelRegistry?: import("piko-host-runtime").ModelRegistry;
  authStorage?: import("piko-host-runtime").AuthStorage;
  sessionName?: string;
  noContextFiles?: boolean;
  noTools?: boolean;
  systemPrompt?: string;
  appendSystemPrompt?: string;
}
