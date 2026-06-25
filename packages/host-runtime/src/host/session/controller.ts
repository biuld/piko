import type { Message } from "piko-orch-protocol";
import type { HostConfig } from "../../models/index.js";
import type {
  PikoSessionRuntime,
  ReplaceSessionEvent,
  SessionManager,
  SessionMeta,
  SessionPersistenceOverview,
  TreeNavigationResult,
} from "../../session/index.js";
import { SessionManager as SessionManagerClass } from "../../session/index.js";
import type { SettingsManager } from "../../settings/index.js";
import type { HostPersistence } from "../persistence/index.js";
import type { HostState } from "../state/index.js";
import {
  type CompactResult,
  generateAutoBranchSummary,
  getEffectiveCompactionSettings,
  runCompact,
  runMaybeCompact,
} from "./compaction.js";

export class HostSessionController {
  constructor(
    private readonly sessionRuntime: PikoSessionRuntime,
    private readonly persistence: HostPersistence,
    private readonly state: HostState,
    private readonly ensureIdle: () => void,
    private readonly getConfig: () => HostConfig,
    private readonly getSettingsManager: () => SettingsManager,
  ) {}

  get sessionManager(): SessionManager {
    return this.sessionRuntime.getSessionManager();
  }

  get sessionId(): string {
    return this.sessionManager.getSessionId();
  }

  get sessionFile(): string | undefined {
    return this.sessionManager.getSessionFile();
  }

  get cwd(): string {
    return this.sessionRuntime.getCwd();
  }

  get diagnostics(): readonly import("../../session/session-runtime.js").SessionRuntimeDiagnostic[] {
    return this.sessionRuntime.diagnostics;
  }

  async refreshPersistenceOverview(): Promise<SessionPersistenceOverview> {
    const overview = await this.persistence.refreshSession();
    this.state.setSessionPersistenceOverview(overview);
    return overview;
  }

  async resetForSession(): Promise<void> {
    const overview = await this.persistence.refreshSession();
    this.state.resetForSession(overview);
  }

  getSessionPersistenceOverview(): SessionPersistenceOverview | undefined {
    return this.state.sessionPersistenceOverview;
  }

  async getSessionName(): Promise<string | undefined> {
    return this.sessionManager.getSessionName();
  }

  async loadMessages(): Promise<Message[]> {
    return this.sessionManager.loadMessages();
  }

  async loadBranchEntries(): ReturnType<SessionManager["loadBranchEntries"]> {
    return this.sessionManager.loadBranchEntries();
  }

  async setSessionName(name?: string): Promise<void> {
    await this.sessionManager.setSessionName(name);
  }

  isSessionPersisted(): boolean {
    return this.sessionManager.isPersisted();
  }

  getParentSessionPath(): string | undefined {
    return this.sessionManager.getParentSessionPath();
  }

  getLeafId(): string | null {
    return this.sessionManager.getLeafId();
  }

  async listSessions(
    options: { scope?: "current" | "all"; namedOnly?: boolean } = {},
  ): Promise<SessionMeta[]> {
    const scope = options.scope ?? "current";
    const sessions =
      scope === "all"
        ? await SessionManagerClass.listAll()
        : await SessionManagerClass.list(this.cwd);
    return options.namedOnly ? sessions.filter((s) => Boolean(s.name)) : sessions;
  }

  async renameSession(specifier: string, name?: string): Promise<boolean> {
    this.ensureIdle();
    return SessionManagerClass.rename(specifier, name, this.cwd);
  }

  async deleteSession(specifier: string): Promise<boolean> {
    this.ensureIdle();
    return SessionManagerClass.delete(specifier, this.cwd);
  }

  async getDivergentMessages(oldLeafId: string | null, newLeafId: string): Promise<number> {
    if (!oldLeafId || oldLeafId === newLeafId) return 0;
    const oldBranch = await this.sessionManager.getBranchFromLeafId(oldLeafId);
    const newBranch = await this.sessionManager.getBranchFromLeafId(newLeafId);
    const newIds = new Set(newBranch.map((e) => e.id));
    return oldBranch.filter((e) => !newIds.has(e.id)).length;
  }

  async getBranchEntries(): Promise<Awaited<ReturnType<SessionManager["getBranch"]>>> {
    return this.sessionManager.getBranch();
  }

  async getTreeEntries(): Promise<Awaited<ReturnType<SessionManager["getTree"]>>> {
    return this.sessionManager.getTree();
  }

  getCompactionSettings() {
    return getEffectiveCompactionSettings(this.getSettingsManager());
  }

  async compact(customInstructions?: string): Promise<CompactResult> {
    const result = await runCompact(
      this.sessionManager,
      this.getConfig(),
      this.getSettingsManager(),
      customInstructions,
    );
    if (!result.compacted && result.error) {
      throw new Error(result.error);
    }
    return result;
  }

  async maybeCompact(): Promise<CompactResult> {
    return runMaybeCompact(this.sessionManager, this.getConfig(), this.getSettingsManager());
  }

  async navigateToEntry(entryId: string): Promise<TreeNavigationResult> {
    this.ensureIdle();
    return this.sessionManager.navigateToEntry(entryId);
  }

  async branchToEntry(entryId: string): Promise<void> {
    this.ensureIdle();
    const summary = await generateAutoBranchSummary(
      this.sessionManager,
      this.getConfig(),
      this.getSettingsManager(),
    );
    if (summary) {
      await this.sessionManager.branchWithSummary(entryId, summary);
    } else {
      await this.sessionManager.branch(entryId);
    }
  }

  async branchToEntryWithSummary(entryId: string, summary: string): Promise<void> {
    await this.sessionManager.branchWithSummary(entryId, summary);
  }

  onSessionReplaced(handler: (event: ReplaceSessionEvent) => Promise<void> | void): void {
    this.sessionRuntime.setOnSessionReplaced(handler);
  }

  onBeforeInvalidate(handler: () => void): void {
    this.sessionRuntime.setOnBeforeInvalidate(handler);
  }

  onAfterRebind(handler: () => Promise<void> | void): void {
    this.sessionRuntime.setOnAfterRebind(handler);
  }

  async switchSession(specifier: string): Promise<SessionManager | null> {
    this.ensureIdle();
    const sessionManager = await this.sessionRuntime.switchSession(specifier);
    if (sessionManager) await this.resetForSession();
    return sessionManager;
  }

  async newSession(options: { parentSession?: string } = {}): Promise<SessionManager> {
    this.ensureIdle();
    const sessionManager = await this.sessionRuntime.newSession(options);
    await this.resetForSession();
    return sessionManager;
  }

  async cloneSession(): Promise<SessionManager> {
    this.ensureIdle();
    const sessionManager = await this.sessionRuntime.cloneSession();
    await this.resetForSession();
    return sessionManager;
  }

  async forkSession(
    entryId: string,
    options?: Parameters<SessionManager["fork"]>[1],
  ): Promise<Awaited<ReturnType<SessionManager["fork"]>>> {
    this.ensureIdle();
    const result = await this.sessionRuntime.forkSession(entryId, options);
    await this.resetForSession();
    return result;
  }

  async importSession(inputPath: string): Promise<SessionManager> {
    const sessionManager = await this.sessionRuntime.importFromJsonl(inputPath);
    await this.resetForSession();
    return sessionManager;
  }

  async dispose(): Promise<void> {
    await this.sessionRuntime.dispose();
  }
}
