export interface RunTuiOptions {
  session?: string;
  settingsManager: import("piko-host-runtime").SettingsManager;
  modelRegistry?: import("piko-host-runtime").ModelRegistry;
  authStorage?: import("piko-host-runtime").AuthStorage;
  sessionName?: string;
  noContextFiles?: boolean;
  noTools?: boolean;
  systemPrompt?: string;
  appendSystemPrompt?: string;
  /** Invoke this prompt template on startup. */
  promptTemplate?: string;
  /** Invoke this skill on startup. */
  skillName?: string;
  /** Path to the current session's debug trace log, if enabled. */
  debugTracePath?: string;
  /** Use Rust hostd as the Host backend for prompt submission. */
  hostd?: {
    enabled: boolean;
    command?: string;
    args?: string[];
  };
}
