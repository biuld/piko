import type { Model, ModelProviderConfig, SettingsManager } from "../shared/index.js";
import { createDefaultSettings, createHostConfig } from "../shared/index.js";

export function createHostdFacade(
  model: Model<string>,
  provider: ModelProviderConfig,
  settingsManager: SettingsManager,
  options: { cwd?: string; session?: string | null; debugTracePath?: string },
): any {
  let config = createHostConfig(model, provider, createDefaultSettings());
  let thinkingLevel = settingsManager.getDefaultThinkingLevel?.();
  const cwd = options.cwd ?? process.cwd();
  const sessionId = options.session ?? "";

  return {
    cwd,
    sessionId,
    sessionFile: sessionId,
    teamMode: false,
    version: "hostd",
    debugTracePath: options.debugTracePath,

    getConfig: () => config,
    setConfig: (next: any) => {
      config = next;
    },
    getThinkingLevel: () => thinkingLevel,
    setThinkingLevel: (level: any) => {
      thinkingLevel = level;
    },
    getSettingsManager: () => settingsManager,
    setLifecycleCallback: () => {},

    restoreFromSession: async () => {},
    loadMessages: async () => [],
    loadBranchEntries: async () => [],
    getSessionName: async () => null,
    setSessionName: async () => {},
    newSession: async () => {},
    cloneSession: async () => {},
    switchSession: async () => null,
    navigateToEntry: async (entryId: string) => ({
      status: "navigated" as const,
      sessionId,
      oldLeafId: null,
      newLeafId: entryId,
      selectedEntryId: entryId,
      branchEntries: [],
    }),
    forkSession: async () => ({}),
    importSession: async () => {},
    renameSession: async () => {},
    listSessions: async () => [],
    getLeafId: async () => undefined,
    getTreeEntries: () => [],
    getContextFiles: () => [],
    getActiveToolNames: () => [],
    getTotalToolCount: () => 0,
    getOrchestratorSnapshot: () => undefined,

    prompt: async () => {},
    dequeue: () => {},
    runSkill: async () => {},
    runPromptTemplate: async () => {},
    compact: async () => ({ message: "Compaction is handled by hostd" }),
    setSteeringMode: () => {},
    setFollowUpMode: () => {},
  };
}
