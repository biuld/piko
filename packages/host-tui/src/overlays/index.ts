import type { TUI } from "@earendil-works/pi-tui";
import type { PikoHost } from "piko-host-runtime";

export interface OverlayContext {
  tui: TUI;
  host: PikoHost;
  msg: (role: string, text: string) => void;
  render: () => void;
  resync: (msg?: string) => Promise<void>;
  doResume: () => Promise<void>;
  doFork: (entryId: string) => Promise<void>;
  setEditorText: (text: string) => void;
  getActiveOverlay(): { hide(): void } | null;
  setActiveOverlay(o: { hide(): void } | null): void;
}

export { openForkSelector } from "./fork-selector.js";
export { openLoginDialog } from "./login-dialog.js";
export { openModelScopeSelector } from "./model-scope-selector.js";
export type { ModelSelectResult } from "./model-selector.js";
export { openModelSelector } from "./model-selector.js";
export { openResumeSelector } from "./resume-selector.js";
export { openSettingsSelector } from "./settings-selector.js";
export { openThinkingSelector } from "./thinking-selector.js";
export { openTreeSelector } from "./tree-selector/index.js";
