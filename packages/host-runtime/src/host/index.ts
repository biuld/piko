import type {
  EngineEvent,
  EngineToolInfo,
  EventStream,
  Message,
  StatelessEngine,
} from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { ApprovalHandler } from "../approval-controller.js";
import type { PromptTemplate } from "../prompts/index.js";
import { loadPromptTemplates } from "../prompts/index.js";
import type { HostConfig } from "../models/index.js";
import type { FollowUpMessage, NextTurnMessage, SteeringMessage } from "../scheduler.js";
import type { SettingsManager } from "../settings/index.js";
import type { Session, SessionMeta } from "../session/index.js";
import {
  PikoSessionRuntime,
  type ReplaceSessionEvent,
  type CreateSessionRuntimeOptions,
  SessionManager,
} from "../session/index.js";
import { loadSkills } from "../skills/index.js";
import { buildEnhancedSystemPromptEngines } from "./system-prompt.js";
import { buildSkillPrompt, buildTemplatePrompt, formatSkillPrompt } from "./skills.js";
import { runCompact, runMaybeCompact, generateAutoBranchSummary } from "./compaction.js";
import { restoreRuntimeFromSession } from "./restore.js";
import {
  runHostPrompt,
  streamHostPrompt,
} from "./run.js";
import type {
  PikoHostCreateOptions,
  StreamPromptOptions,
  StreamPromptResult,
  HostRunResult,
} from "./types.js";

// Re-export helpers
export { formatSkillPrompt } from "./skills.js";

// ---- Types (re-exported) ----
export type {
  PikoHostCreateOptions,
  StreamPromptOptions,
  StreamPromptResult,
  HostRunResult,
} from "./types.js";

// ---- Host ----

export class PikoHost {
  private engine: StatelessEngine;
  private config: HostConfig;
  private approvalHandler?: ApprovalHandler;
  private systemPrompt: string;
  private sessionRuntime: PikoSessionRuntime;
  private settingsManager?: SettingsManager;
  private _thinkingLevel: string = "off";
  private steeringQueue: SteeringMessage[] = [];
  private followUpQueue: FollowUpMessage[] = [];
  private nextTurnQueue: NextTurnMessage[] = [];
  private _skills: ReturnType<typeof loadSkills>["skills"] = [];
  private _promptTemplates: PromptTemplate[] = [];

  constructor(
    engine: StatelessEngine,
    config: HostConfig,
    sessionRuntime: PikoSessionRuntime,
    options: {
      approvalHandler?: ApprovalHandler;
      systemPrompt?: string;
      appendSystemPrompt?: string;
      promptGuidelines?: string[];
      promptTemplates?: PromptTemplate[];
      settingsManager?: SettingsManager;
      skipContextFiles?: boolean;
    } = {},
  ) {
    this.engine = engine;
    this.config = config;
    this.approvalHandler = options.approvalHandler;
    this.settingsManager = options.settingsManager;
    const cwd = sessionRuntime.getCwd();
    this.systemPrompt = options.systemPrompt ??
      buildEnhancedSystemPromptEngines(
        this.engine.capabilities.tools,
        cwd,
        options.appendSystemPrompt,
        options.promptGuidelines,
        options.promptTemplates,
        options.skipContextFiles,
      );
    this.sessionRuntime = sessionRuntime;

    const skillsResult = loadSkills({ cwd });
    this._skills = skillsResult.skills;
    this._promptTemplates = options.promptTemplates ?? loadPromptTemplates({ cwd });
  }

  // ---- P0: Runtime Model Switching ----

  setConfig(config: HostConfig): void {
    const oldModel = this.config.model;
    this.config = config;
    if (config.model.provider !== oldModel.provider || config.model.id !== oldModel.id) {
      this.sessionManager.appendModelChange(config.model.provider, config.model.id).catch(() => {});
    }
  }

  getConfig(): HostConfig { return this.config; }

  setThinkingLevel(level: string): void {
    if (this._thinkingLevel !== level) {
      this._thinkingLevel = level;
      this.sessionManager.appendThinkingLevelChange(level).catch(() => {});
    }
  }

  getThinkingLevel(): string { return this._thinkingLevel; }

  // ---- P1: Agent Loop APIs ----

  steer(text: string): void { this.steeringQueue.push({ text }); }
  followUp(text: string): void { this.followUpQueue.push({ text }); }
  nextTurn(text: string): void { this.nextTurnQueue.push({ text }); }

  // ---- P2: Skills & Templates ----

  async runSkill(name: string, additionalInstructions?: string, signal?: AbortSignal): Promise<HostRunResult> {
    const prompt = buildSkillPrompt(this._skills, name, additionalInstructions);
    return this.run(prompt, signal);
  }

  streamSkill(name: string, additionalInstructions?: string, signal?: AbortSignal): EventStream<EngineEvent, StreamPromptResult> {
    try {
      const prompt = buildSkillPrompt(this._skills, name, additionalInstructions);
      return this.streamPrompt(prompt, {}, signal);
    } catch (e: unknown) {
      const s = new EventStreamImpl<EngineEvent, StreamPromptResult>();
      s.push({ type: "error", message: e instanceof Error ? e.message : String(e) });
      s.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      return s;
    }
  }

  async runPromptTemplate(name: string, args: string[] = [], signal?: AbortSignal): Promise<HostRunResult> {
    const prompt = buildTemplatePrompt(this._promptTemplates, name, args);
    return this.run(prompt, signal);
  }

  streamPromptTemplate(name: string, args: string[] = [], signal?: AbortSignal): EventStream<EngineEvent, StreamPromptResult> {
    try {
      const prompt = buildTemplatePrompt(this._promptTemplates, name, args);
      return this.streamPrompt(prompt, {}, signal);
    } catch (e: unknown) {
      const s = new EventStreamImpl<EngineEvent, StreamPromptResult>();
      s.push({ type: "error", message: e instanceof Error ? e.message : String(e) });
      s.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      return s;
    }
  }

  get skills(): ReturnType<typeof loadSkills>["skills"] { return this._skills; }
  get promptTemplates(): PromptTemplate[] { return this._promptTemplates; }

  // ---- Session state restoration ----

  async restoreFromSession(): Promise<void> {
    const result = await restoreRuntimeFromSession(this.sessionManager, this.config);
    if (result.config) this.config = result.config;
    if (result.thinkingLevel !== undefined) this._thinkingLevel = result.thinkingLevel;
  }

  // ---- Compaction & Branch Summary ----

  getCompactionSettings() { return this.settingsManager?.getCompactionSettings() ?? { enabled: true, reserveTokens: 16384, keepRecentTokens: 20000 }; }

  async compact(customInstructions?: string): Promise<void> {
    return runCompact(this.sessionManager, this.config, this.settingsManager, customInstructions);
  }

  async maybeCompact(): Promise<void> {
    return runMaybeCompact(this.sessionManager, this.config, this.settingsManager);
  }

  async branchToEntry(entryId: string): Promise<void> {
    const summary = await generateAutoBranchSummary(this.sessionManager, this.config, this.settingsManager);
    if (summary) {
      await this.sessionManager.branchWithSummary(entryId, summary);
    } else {
      await this.sessionManager.branch(entryId);
    }
  }

  async branchToEntryWithSummary(entryId: string, summary: string): Promise<void> {
    await this.sessionManager.branchWithSummary(entryId, summary);
  }

  // ---- Engine info ----

  get availableTools(): EngineToolInfo[] { return this.engine.capabilities.tools; }

  // ---- Factories (static) ----

  static async create(options: PikoHostCreateOptions): Promise<PikoHost> {
    const { createPikoHost } = await import("./factories.js");
    return createPikoHost(options);
  }

  static fromSessionManager(
    engine: StatelessEngine, config: HostConfig, sessionManager: SessionManager,
    options: { approvalHandler?: ApprovalHandler; systemPrompt?: string; settingsManager?: SettingsManager } = {},
  ): PikoHost {
    const sessionRuntime = PikoSessionRuntime.fromSessionManager(sessionManager);
    return new PikoHost(engine, config, sessionRuntime, options);
  }

  // ---- Session accessors ----

  getSettingsManager(): SettingsManager | undefined { return this.settingsManager; }
  get sessionManager(): SessionManager { return this.sessionRuntime.getSessionManager(); }
  get sessionId(): string { return this.sessionManager.getSessionId(); }
  get sessionFile(): string | undefined { return this.sessionManager.getSessionFile(); }
  get cwd(): string { return this.sessionRuntime.getCwd(); }

  async getSessionName(): Promise<string | undefined> { return this.sessionManager.getSessionName(); }
  async loadMessages(): Promise<Message[]> { return this.sessionManager.loadMessages(); }
  async setSessionName(name?: string): Promise<void> { await this.sessionManager.setSessionName(name); }
  isSessionPersisted(): boolean { return this.sessionManager.isPersisted(); }
  getParentSessionPath(): string | undefined { return this.sessionManager.getParentSessionPath(); }
  getLeafId(): string | null { return this.sessionManager.getLeafId(); }

  async listSessions(options: { scope?: "current" | "all"; namedOnly?: boolean } = {}): Promise<SessionMeta[]> {
    const scope = options.scope ?? "current";
    const sessions = scope === "all" ? await SessionManager.listAll() : await SessionManager.list(this.cwd);
    return options.namedOnly ? sessions.filter((s) => Boolean(s.name)) : sessions;
  }

  async renameSession(specifier: string, name?: string): Promise<boolean> { return SessionManager.rename(specifier, name, this.cwd); }
  async deleteSession(specifier: string): Promise<boolean> { return SessionManager.delete(specifier, this.cwd); }

  async getDivergentMessages(oldLeafId: string | null, newLeafId: string): Promise<number> {
    if (!oldLeafId || oldLeafId === newLeafId) return 0;
    const oldBranch = await this.sessionManager.getBranchFromLeafId(oldLeafId);
    const newBranch = await this.sessionManager.getBranchFromLeafId(newLeafId);
    const newIds = new Set(newBranch.map((e) => e.id));
    return oldBranch.filter((e) => !newIds.has(e.id)).length;
  }

  async getBranchEntries(): Promise<Awaited<ReturnType<SessionManager["getBranch"]>>> { return this.sessionManager.getBranch(); }
  async getTreeEntries(): Promise<Awaited<ReturnType<SessionManager["getTree"]>>> { return this.sessionManager.getTree(); }

  get diagnostics(): readonly import("../session/session-runtime.js").SessionRuntimeDiagnostic[] { return this.sessionRuntime.diagnostics; }
  onSessionReplaced(handler: (event: ReplaceSessionEvent) => Promise<void> | void): void { this.sessionRuntime.setOnSessionReplaced(handler); }
  onBeforeInvalidate(handler: () => void): void { this.sessionRuntime.setOnBeforeInvalidate(handler); }
  onAfterRebind(handler: () => Promise<void> | void): void { this.sessionRuntime.setOnAfterRebind(handler); }

  async switchSession(specifier: string): Promise<SessionManager | null> { return this.sessionRuntime.switchSession(specifier); }
  async newSession(options: { parentSession?: string } = {}): Promise<SessionManager> { return this.sessionRuntime.newSession(options); }
  async cloneSession(): Promise<SessionManager> { return this.sessionRuntime.cloneSession(); }

  async forkSession(entryId: string, options?: Parameters<SessionManager["fork"]>[1]): Promise<Awaited<ReturnType<SessionManager["fork"]>>> {
    return this.sessionRuntime.forkSession(entryId, options);
  }

  async importSession(inputPath: string): Promise<SessionManager> { return this.sessionRuntime.importFromJsonl(inputPath); }
  async dispose(): Promise<void> { await this.sessionRuntime.dispose(); }

  // ---- Run (multi-step, non-streaming) ----

  async run(prompt: string, signal?: AbortSignal): Promise<HostRunResult> {
    return runHostPrompt(
      this.engine, this.config, this.sessionManager, this.systemPrompt,
      this.settingsManager, this.approvalHandler, this._thinkingLevel,
      this.steeringQueue, this.followUpQueue, this.nextTurnQueue,
      prompt, signal,
    );
  }

  // ---- Stream prompt (multi-step, streaming) ----

  streamPrompt(prompt: string, options: StreamPromptOptions = {}, signal?: AbortSignal): EventStream<EngineEvent, StreamPromptResult> {
    return streamHostPrompt(
      this.engine, this.config, this.sessionManager, this.systemPrompt,
      this.settingsManager, this.approvalHandler, this._thinkingLevel,
      this.steeringQueue, this.followUpQueue, this.nextTurnQueue,
      prompt, options, signal,
    );
  }
}
