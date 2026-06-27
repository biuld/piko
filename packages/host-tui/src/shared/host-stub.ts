// ============================================================================
// Minimal PikoHost stub — provides Pick<PikoHost, ...> compatibility for TuiHostFacade.
// All methods match the signatures action-service.ts and other TUI code expects.
// No runtime implementation — hostd handles everything.
// ============================================================================

// eslint-disable-next-line @typescript-eslint/no-extraneous-class
export class PikoHost {
  static async create(_options: any): Promise<PikoHost> {
    throw new Error("PikoHost.create is unavailable — use hostd mode");
  }

  cwd: any;
  sessionId: any;
  sessionFile: any;
  teamMode: any;
  version: any;
  debugTracePath: any;

  getConfig(): any {
    throw new Error("stub");
  }
  setConfig(_config: any): void {}
  getThinkingLevel(): any {
    return undefined;
  }
  setThinkingLevel(_level: any): void {}
  getSettingsManager(): any {
    throw new Error("stub");
  }
  setLifecycleCallback(_cb: any): void {}
  restoreFromSession(): Promise<any> {
    return Promise.resolve();
  }
  loadMessages(): Promise<any> {
    return Promise.resolve([]);
  }
  loadBranchEntries(): Promise<any> {
    return Promise.resolve([]);
  }
  getSessionName(): Promise<any> {
    return Promise.resolve(undefined);
  }
  setSessionName(_name: any): Promise<any> {
    return Promise.resolve();
  }
  newSession(): Promise<any> {
    return Promise.resolve();
  }
  cloneSession(_name?: any): Promise<any> {
    return Promise.resolve();
  }
  switchSession(_sessionId: any, _entryId?: any): Promise<any> {
    return Promise.resolve({});
  }
  navigateToEntry(_entryId: any, _signal?: any): Promise<any> {
    throw new Error("stub");
  }
  forkSession(_entryId?: any): Promise<any> {
    return Promise.resolve({});
  }
  importSession(_path: any): Promise<any> {
    return Promise.resolve();
  }
  renameSession(..._args: any[]): Promise<any> {
    return Promise.resolve();
  }
  listSessions(..._args: any[]): Promise<any> {
    return Promise.resolve([]);
  }
  getLeafId(): Promise<any> {
    return Promise.resolve(null);
  }
  getTreeEntries(): any {
    return [];
  }
  getContextFiles(): any {
    return [];
  }
  getActiveToolNames(): any {
    return [];
  }
  getTotalToolCount(): any {
    return 0;
  }
  getOrchestratorSnapshot(): any {
    return undefined;
  }

  prompt(..._args: any[]): any {
    const it: any = {
      [Symbol.asyncIterator]: () => ({ next: async () => ({ done: true, value: undefined }) }),
    };
    it.result = Promise.resolve({ messages: [] });
    return it;
  }
  dequeue(..._args: any[]): any {
    return { steering: [], followUp: [], nextTurn: [] };
  }
  runSkill(..._args: any[]): Promise<any> {
    return Promise.resolve();
  }
  runPromptTemplate(..._args: any[]): Promise<any> {
    return Promise.resolve();
  }
  compact(): Promise<any> {
    return Promise.resolve({});
  }
  setSteeringMode(_enabled: any): void {}
  setFollowUpMode(_enabled: any): void {}

  private constructor() {}
}
