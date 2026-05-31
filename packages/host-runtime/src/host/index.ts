import type {
  EngineEvent,
  EngineToolInfo,
  EventStream,
  ImageContent,
  Message,
  StatelessEngine,
} from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { ApprovalHandler } from "../approval-controller.js";
import type { HostConfig } from "../models/index.js";
import type { PromptTemplate } from "../prompts/index.js";
import { loadPromptTemplates } from "../prompts/index.js";
import type { FollowUpMessage, NextTurnMessage, QueueMode, SteeringMessage } from "../scheduler.js";
import type { SessionMeta } from "../session/index.js";
import { PikoSessionRuntime, type ReplaceSessionEvent, SessionManager } from "../session/index.js";
import type { SettingsManager } from "../settings/index.js";
import { loadSkills } from "../skills/index.js";
import {
  type ActiveToolsState,
  activeToolNamesFromState,
  activeToolsStateFromNames,
} from "../turn-state.js";
import { generateAutoBranchSummary, runCompact, runMaybeCompact } from "./compaction.js";
import { restoreRuntimeFromSession } from "./restore.js";
import { createPrepareNextTurn, runHostPrompt, streamHostPrompt } from "./run.js";
import { buildSkillPrompt, buildTemplatePrompt } from "./skills.js";
import { buildEnhancedSystemPromptEngines } from "./system-prompt.js";
import type {
  HostRunResult,
  PikoHostCreateOptions,
  StreamPromptOptions,
  StreamPromptResult,
} from "./types.js";

// Re-export lifecycle events
export type {
  AgentEndEvent,
  AgentStartEvent,
  FailureEvent,
  HostLifecycleEvent,
  MessageEndEvent,
  MessageStartEvent,
  MessageUpdateEvent,
  QueueUpdateEvent,
  SavePointEvent,
  SettledEvent,
  ToolExecutionEndEvent,
  ToolExecutionStartEvent,
  ToolExecutionUpdateEvent,
  TurnEndEvent,
  TurnStartEvent,
} from "./lifecycle-events.js";
export { createPrepareNextTurn } from "./run.js";
// Re-export helpers
export { formatSkillPrompt } from "./skills.js";

// ---- Types (re-exported) ----
export type {
  HostRunResult,
  PikoHostCreateOptions,
  StreamPromptOptions,
  StreamPromptResult,
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
  private _activeToolsState: ActiveToolsState = { kind: "all" };
  private steeringQueue: SteeringMessage[] = [];
  private followUpQueue: FollowUpMessage[] = [];
  private nextTurnQueue: NextTurnMessage[] = [];
  private steeringMode: QueueMode = "all";
  private followUpMode: QueueMode = "one-at-a-time";
  private _skills: ReturnType<typeof loadSkills>["skills"] = [];
  private _promptTemplates: PromptTemplate[] = [];

  /**
   * Current run phase. Used to validate queue operations.
   * - "idle": No run in progress. steer()/followUp() are rejected.
   * - "running": A prompt/skill/template run is in progress.
   */
  private _phase: "idle" | "running" = "idle";

  /**
   * Throw if a run is in progress. Session mutations (branch, fork, switch)
   * require an idle harness to avoid conflicts with in-progress writes.
   */
  private _ensureIdle(): void {
    if (this._phase !== "idle") {
      throw new Error("Cannot perform session mutation while a run is in progress");
    }
  }

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
    // Populate config.tools from engine capabilities if not already set.
    // This enables active tools filtering (skill tools: metadata) in the
    // normal TUI/CLI path where tool definitions are not passed via HostConfig.
    if (!this.config.tools?.length && engine.capabilities.engineTools?.length) {
      this.config = {
        ...this.config,
        tools: engine.capabilities.engineTools,
      };
    }
    this.approvalHandler = options.approvalHandler;
    this.settingsManager = options.settingsManager;
    const cwd = sessionRuntime.getCwd();
    this.systemPrompt =
      options.systemPrompt ??
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
    // Preserve tools from engine if not provided in the new config.
    // This prevents losing active tools filtering after model switches.
    if (!config.tools?.length && this.config.tools?.length) {
      config = { ...config, tools: this.config.tools };
    }
    this.config = config;
    if (config.model.provider !== oldModel.provider || config.model.id !== oldModel.id) {
      this.sessionManager.appendModelChange(config.model.provider, config.model.id).catch(() => {});
    }
  }

  getConfig(): HostConfig {
    return this.config;
  }

  setThinkingLevel(level: string): void {
    if (this._thinkingLevel !== level) {
      this._thinkingLevel = level;
      this.sessionManager.appendThinkingLevelChange(level).catch(() => {});
    }
  }

  getThinkingLevel(): string {
    return this._thinkingLevel;
  }

  getActiveToolNames(): string[] | undefined {
    return activeToolNamesFromState(this._activeToolsState);
  }

  setActiveToolNames(toolNames: string[] | undefined): void {
    this._activeToolsState = activeToolsStateFromNames(toolNames);
    // Always persist: explicit clear (undefined -> all tools) or explicit set.
    this.sessionManager.appendActiveToolsChange(this.getActiveToolNames() ?? []).catch(() => {});
  }

  // ---- P1: Agent Loop APIs ----

  /**
   * Queue a steering message to inject during the current run.
   * Rejects if no run is in progress (phase !== "running").
   */
  steer(text: string, images?: ImageContent[]): void {
    if (this._phase !== "running") {
      throw new Error("Cannot steer while idle");
    }
    this.steeringQueue.push({ text, images });
  }

  /**
   * Queue a follow-up message to run after the current turn completes.
   * Rejects if no run is in progress (phase !== "running").
   */
  followUp(text: string, images?: ImageContent[]): void {
    if (this._phase !== "running") {
      throw new Error("Cannot follow up while idle");
    }
    this.followUpQueue.push({ text, images });
  }

  /**
   * Queue a message for the next full turn. Can be called anytime.
   */
  nextTurn(text: string, images?: ImageContent[]): void {
    this.nextTurnQueue.push({ text, images });
  }

  // ---- P2: Skills & Templates ----

  async runSkill(
    name: string,
    additionalInstructions?: string,
    signal?: AbortSignal,
  ): Promise<HostRunResult> {
    const skill = this._skills.find((s) => s.name === name);
    if (!skill) throw new Error(`Unknown skill: ${name}`);

    // Apply skill metadata overrides
    const prevModel = this.config.model;
    const prevThinking = this._thinkingLevel;
    const prevActiveTools = this._activeToolsState;
    if (skill.modelOverride) {
      const [provider, modelId] = skill.modelOverride.split("/");
      if (provider && modelId) {
        this.config = { ...this.config, model: { ...this.config.model, provider, id: modelId } };
      }
    }
    if (skill.thinkingLevel) {
      this._thinkingLevel = skill.thinkingLevel;
    }
    if (skill.activeTools !== undefined) {
      this._activeToolsState = activeToolsStateFromNames(
        skill.activeTools
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
      );
    }

    try {
      const prompt = buildSkillPrompt(this._skills, name, additionalInstructions);
      return await this.run(prompt, signal);
    } finally {
      this.config = { ...this.config, model: prevModel };
      this._thinkingLevel = prevThinking;
      this._activeToolsState = prevActiveTools;
    }
  }

  streamSkill(
    name: string,
    additionalInstructions?: string,
    signal?: AbortSignal,
  ): EventStream<EngineEvent, StreamPromptResult> {
    try {
      const skill = this._skills.find((s) => s.name === name);
      if (!skill) throw new Error(`Unknown skill: ${name}`);

      // Apply skill metadata overrides
      const prevModel = this.config.model;
      const prevThinking = this._thinkingLevel;
      const prevActiveTools = this._activeToolsState;
      if (skill.modelOverride) {
        const [provider, modelId] = skill.modelOverride.split("/");
        if (provider && modelId) {
          this.config = { ...this.config, model: { ...this.config.model, provider, id: modelId } };
        }
      }
      if (skill.thinkingLevel) {
        this._thinkingLevel = skill.thinkingLevel;
      }
      if (skill.activeTools !== undefined) {
        this._activeToolsState = activeToolsStateFromNames(
          skill.activeTools
            .split(",")
            .map((t) => t.trim())
            .filter(Boolean),
        );
      }

      const prompt = buildSkillPrompt(this._skills, name, additionalInstructions);
      const resultStream = this.streamPrompt(prompt, {}, signal);

      // Restore overrides when stream ends
      const originalEnd = resultStream.end.bind(resultStream);
      resultStream.end = (value: StreamPromptResult) => {
        this.config = { ...this.config, model: prevModel };
        this._thinkingLevel = prevThinking;
        this._activeToolsState = prevActiveTools;
        return originalEnd(value);
      };
      return resultStream;
    } catch (e: unknown) {
      const s = new EventStreamImpl<EngineEvent, StreamPromptResult>();
      s.push({ type: "error", message: e instanceof Error ? e.message : String(e) });
      s.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      return s;
    }
  }

  async runPromptTemplate(
    name: string,
    args: string[] = [],
    signal?: AbortSignal,
  ): Promise<HostRunResult> {
    const prompt = buildTemplatePrompt(this._promptTemplates, name, args);
    return this.run(prompt, signal);
  }

  streamPromptTemplate(
    name: string,
    args: string[] = [],
    signal?: AbortSignal,
  ): EventStream<EngineEvent, StreamPromptResult> {
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

  get skills(): ReturnType<typeof loadSkills>["skills"] {
    return this._skills;
  }
  get promptTemplates(): PromptTemplate[] {
    return this._promptTemplates;
  }

  // ---- Session state restoration ----

  async restoreFromSession(): Promise<void> {
    const result = await restoreRuntimeFromSession(this.sessionManager, this.config);
    if (result.config) this.config = result.config;
    if (result.thinkingLevel !== undefined) this._thinkingLevel = result.thinkingLevel;
    this._activeToolsState = activeToolsStateFromNames(
      result.hasActiveToolsEntry ? result.activeToolNames : undefined,
    );
  }

  // ---- Compaction & Branch Summary ----

  getCompactionSettings() {
    return (
      this.settingsManager?.getCompactionSettings() ?? {
        enabled: true,
        reserveTokens: 16384,
        keepRecentTokens: 20000,
      }
    );
  }

  async compact(customInstructions?: string): Promise<void> {
    return runCompact(this.sessionManager, this.config, this.settingsManager, customInstructions);
  }

  async maybeCompact(): Promise<void> {
    return runMaybeCompact(this.sessionManager, this.config, this.settingsManager);
  }

  async branchToEntry(entryId: string): Promise<void> {
    this._ensureIdle();
    const summary = await generateAutoBranchSummary(
      this.sessionManager,
      this.config,
      this.settingsManager,
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

  // ---- Engine info ----

  get availableTools(): EngineToolInfo[] {
    return this.engine.capabilities.tools;
  }

  // ---- Factories (static) ----

  static async create(options: PikoHostCreateOptions): Promise<PikoHost> {
    const { createPikoHost } = await import("./factories.js");
    return createPikoHost(options);
  }

  static fromSessionManager(
    engine: StatelessEngine,
    config: HostConfig,
    sessionManager: SessionManager,
    options: {
      approvalHandler?: ApprovalHandler;
      systemPrompt?: string;
      settingsManager?: SettingsManager;
    } = {},
  ): PikoHost {
    const sessionRuntime = PikoSessionRuntime.fromSessionManager(sessionManager);
    return new PikoHost(engine, config, sessionRuntime, options);
  }

  // ---- Session accessors ----

  getSettingsManager(): SettingsManager | undefined {
    return this.settingsManager;
  }
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

  async getSessionName(): Promise<string | undefined> {
    return this.sessionManager.getSessionName();
  }
  async loadMessages(): Promise<Message[]> {
    return this.sessionManager.loadMessages();
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
      scope === "all" ? await SessionManager.listAll() : await SessionManager.list(this.cwd);
    return options.namedOnly ? sessions.filter((s) => Boolean(s.name)) : sessions;
  }

  async renameSession(specifier: string, name?: string): Promise<boolean> {
    this._ensureIdle();
    return SessionManager.rename(specifier, name, this.cwd);
  }
  async deleteSession(specifier: string): Promise<boolean> {
    this._ensureIdle();
    return SessionManager.delete(specifier, this.cwd);
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

  get diagnostics(): readonly import("../session/session-runtime.js").SessionRuntimeDiagnostic[] {
    return this.sessionRuntime.diagnostics;
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
    this._ensureIdle();
    return this.sessionRuntime.switchSession(specifier);
  }
  async newSession(options: { parentSession?: string } = {}): Promise<SessionManager> {
    this._ensureIdle();
    return this.sessionRuntime.newSession(options);
  }
  async cloneSession(): Promise<SessionManager> {
    this._ensureIdle();
    return this.sessionRuntime.cloneSession();
  }

  async forkSession(
    entryId: string,
    options?: Parameters<SessionManager["fork"]>[1],
  ): Promise<Awaited<ReturnType<SessionManager["fork"]>>> {
    this._ensureIdle();
    return this.sessionRuntime.forkSession(entryId, options);
  }

  async importSession(inputPath: string): Promise<SessionManager> {
    return this.sessionRuntime.importFromJsonl(inputPath);
  }
  async dispose(): Promise<void> {
    await this.sessionRuntime.dispose();
  }

  // ---- Run (multi-step, non-streaming) ----

  async run(prompt: string, signal?: AbortSignal): Promise<HostRunResult> {
    this._phase = "running";
    try {
      return await runHostPrompt(
        this.engine,
        this.config,
        this.sessionManager,
        this.systemPrompt,
        this.settingsManager,
        this.approvalHandler,
        this.steeringQueue,
        this.followUpQueue,
        this.nextTurnQueue,
        prompt,
        createPrepareNextTurn(
          () => this.config,
          () => this._thinkingLevel,
          () => this.systemPrompt,
          () => this._activeToolsState,
        ),
        signal,
        this.steeringMode,
        this.followUpMode,
      );
    } finally {
      this._phase = "idle";
    }
  }

  // ---- Stream prompt (multi-step, streaming) ----

  streamPrompt(
    prompt: string,
    options: StreamPromptOptions = {},
    signal?: AbortSignal,
  ): EventStream<EngineEvent, StreamPromptResult> {
    this._phase = "running";
    const resultStream = streamHostPrompt(
      this.engine,
      this.config,
      this.sessionManager,
      this.systemPrompt,
      this.settingsManager,
      this.approvalHandler,
      this.steeringQueue,
      this.followUpQueue,
      this.nextTurnQueue,
      prompt,
      options,
      createPrepareNextTurn(
        () => this.config,
        () => this._thinkingLevel,
        () => this.systemPrompt,
        () => this._activeToolsState,
      ),
      signal,
      this.steeringMode,
      this.followUpMode,
    );

    // Clear phase when stream settles (success or error)
    const originalEnd = resultStream.end.bind(resultStream);
    resultStream.end = (value: StreamPromptResult) => {
      this._phase = "idle";
      return originalEnd(value);
    };
    return resultStream;
  }
}
