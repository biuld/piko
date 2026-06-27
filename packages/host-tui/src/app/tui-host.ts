import type {
  ContextFile,
  FlatTreeEntry,
  HostConfig,
  OrchState,
  PikoHost,
} from "piko-host-runtime";

export type TuiHostFacade = Pick<
  PikoHost,
  | "cwd"
  | "sessionId"
  | "sessionFile"
  | "teamMode"
  | "version"
  | "debugTracePath"
  | "getConfig"
  | "setConfig"
  | "getThinkingLevel"
  | "setThinkingLevel"
  | "getSettingsManager"
  | "setLifecycleCallback"
  | "restoreFromSession"
  | "loadMessages"
  | "loadBranchEntries"
  | "getSessionName"
  | "setSessionName"
  | "newSession"
  | "cloneSession"
  | "switchSession"
  | "navigateToEntry"
  | "forkSession"
  | "importSession"
  | "renameSession"
  | "listSessions"
  | "getLeafId"
  | "getTreeEntries"
  | "getContextFiles"
  | "getActiveToolNames"
  | "getTotalToolCount"
  | "getOrchestratorSnapshot"
  | "prompt"
  | "dequeue"
  | "runSkill"
  | "runPromptTemplate"
  | "compact"
  | "setSteeringMode"
  | "setFollowUpMode"
>;

export type TuiHostConfig = HostConfig;
export type TuiOrchState = OrchState;
export type TuiContextFile = ContextFile;
export type TuiFlatTreeEntry = FlatTreeEntry;
