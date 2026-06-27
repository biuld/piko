import type { Model, ModelProviderConfig, SettingsManager } from "piko-host-runtime";
import { createDefaultSettings, createHostConfig } from "piko-host-runtime";

export function createHostdFacade(
  model: Model<string>,
  provider: ModelProviderConfig,
  settingsManager: SettingsManager,
  options: { cwd?: string; session?: string | null; debugTracePath?: string },
): any {
  let config = createHostConfig(model, provider, createDefaultSettings());
  let thinkingLevel = settingsManager.getDefaultThinkingLevel?.();
  const cwd = options.cwd ?? process.cwd();

  return {
    cwd,
    sessionId: options.session ?? "",
    sessionFile: options.session ?? "",
    teamMode: false,
    version: "hostd",
    debugTracePath: options.debugTracePath,

    getConfig: () => config,
    setConfig: (next: typeof config) => {
      config = next;
    },
    getThinkingLevel: () => thinkingLevel,
    setThinkingLevel: (level: string) => {
      thinkingLevel = level as any;
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
      status: "navigated",
      sessionId: options.session ?? "",
      oldLeafId: null,
      newLeafId: entryId,
      selectedEntryId: entryId,
      branchEntries: [],
    }),
    forkSession: async () => ({}),
    importSession: async () => {},
    renameSession: async () => {},
    listSessions: async () => [],
    getLeafId: async () => null,
    getTreeEntries: async () => [],
    getContextFiles: async () => [],
    getActiveToolNames: () => undefined,
    getTotalToolCount: () => 0,
    getOrchestratorSnapshot: () => undefined,
    prompt: () => {
      throw new Error("PikoHost prompt is unavailable in hostd mode");
    },
    dequeue: () => ({ steering: [], followUp: [], nextTurn: [] }),
    runSkill: async () => {},
    runPromptTemplate: async () => {},
    compact: async () => ({ message: "Compaction is handled by hostd" }),
    setSteeringMode: () => {},
    setFollowUpMode: () => {},
  };
}
