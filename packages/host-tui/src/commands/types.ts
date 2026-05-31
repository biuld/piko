import type { PikoHost, SessionMeta } from "piko-host-runtime";

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
  doFork: (entryId: string) => Promise<void>;
  doResumeSelector: () => Promise<void>;
  doModelSelector: () => Promise<void>;
  doThinkingSelector: () => Promise<void>;
  doSettingsSelector: () => Promise<void>;
  doModelScopeSelector: () => Promise<void>;
  doLoginSelector: (provider: string) => Promise<void>;
  cycleModelForward: () => Promise<void>;
  cycleModelBackward: () => Promise<void>;
  thinkingLevel: string;
  setThinkingLevel: (level: string) => void;
  /** Set editor text (for /template, /skill, etc.). */
  setEditorText?: (text: string) => void;
  /** Submit a user message as if the user typed and submitted it (streaming). */
  submitUserMessage?: (text: string) => void;
  /** Submit a stream created by a factory that receives an AbortSignal (fix #1 — supports Ctrl+C abort). */
  submitStream?: (
    factory: (signal: AbortSignal) => ReturnType<PikoHost["streamPrompt"]>,
    displayText: string,
  ) => void;
  listModels: () => { provider: string; models: { id: string; name: string }[] }[];
  formatSessions: (sessions: SessionMeta[]) => string[];
  switchTheme: (name: string) => boolean;
  currentTheme: string;
  /** Reload settings, skills, templates and refresh runtime state. */
  reloadRuntime?: () => Promise<void>;
}
