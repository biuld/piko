import type { PikoHost } from "piko-host-runtime";

export interface Command {
  value: string;
  label: string;
  description: string;
}

/** The closures and state that slash commands need from the TUI. */
export interface CommandContext {
  host: PikoHost;
  model: { provider: string; id: string; name: string };
  sessionName: string | undefined;
  setSessionName: (name: string | undefined) => void;
  transcriptLength: number;
  msg: (role: string, text: string) => void;
  render: () => void;
  refreshHeader: () => void;
  refreshFooter: () => void;
  resync: (sysMsg?: string) => Promise<void>;
  doResume: () => Promise<void>;
  doNewSession: () => Promise<void>;
  doTreeSelector: () => Promise<void>;
  doForkSelector: () => Promise<void>;
  doClone: () => Promise<void>;
  doResumeSelector: () => Promise<void>;
  doModelSelector: (search?: string) => Promise<void>;
  doSettingsSelector: () => Promise<void>;
  doModelScopeSelector: () => Promise<void>;
  doLoginSelector: (provider: string) => Promise<void>;
  cycleModelForward: () => Promise<void>;
  cycleModelBackward: () => Promise<void>;
  thinkingLevel: string;
  setThinkingLevel: (level: string) => void;
  submitStream?: (
    factory: (signal: AbortSignal) => ReturnType<PikoHost["streamPrompt"]>,
    displayText: string,
    kind?: "skill" | "template",
  ) => void;
  /** Reload settings, skills, templates and refresh runtime state. */
  reloadRuntime?: () => Promise<void>;
}
