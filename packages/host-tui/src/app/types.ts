import type { TuiModelCatalog } from "./model-catalog.js";
import type { TuiPreferences } from "./tui-preferences.js";

export interface RunTuiOptions {
  session?: string;
  preferences: TuiPreferences;
  modelCatalog?: TuiModelCatalog;
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
