import { createNativeEngine } from "piko-engine-native";
import type {
  EngineEvent,
  EngineRunSettings,
  EngineTool,
  EngineToolInfo,
  EventStream,
  Message,
  StatelessEngine,
} from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { ApprovalHandler } from "./approval-controller.js";
import {
  type CompactionSettings,
  compact,
  estimateContextTokens,
  generateBranchSummary,
  prepareCompaction,
  shouldCompact,
} from "./compaction/index.js";
import type { HostConfig } from "./models/index.js";
import {
  buildSystemPrompt,
  loadContextFiles,
  loadPromptTemplates,
  type PromptTemplate,
  substituteArgs,
} from "./prompts/index.js";
import type {
  FollowUpMessage,
  NextTurnMessage,
  SteeringMessage,
  TurnPreparation,
} from "./scheduler.js";
import { runScheduler } from "./scheduler.js";
import type { Session, SessionMeta } from "./session/index.js";
import {
  addUserMessage,
  type CreateSessionRuntimeOptions,
  createSession,
  getSessionDir,
  JsonlSessionRepo,
  NodeExecutionEnv,
  PikoSessionRuntime,
  type ReplaceSessionEvent,
  SessionManager,
} from "./session/index.js";
import type { SettingsManager } from "./settings/index.js";
import { loadSkills } from "./skills/index.js";

// ---- Options ----

export interface PikoHostCreateOptions {
  /** Engine implementation. Defaults to native engine with pi-ai LLM caller. */
  engine?: StatelessEngine;
  config: HostConfig;
  approvalHandler?: ApprovalHandler;
  systemPrompt?: string;
  session?: CreateSessionRuntimeOptions;
  /** Append to system prompt (after default). */
  appendSystemPrompt?: string;
  /** Custom guidelines for the system prompt. */
  promptGuidelines?: string[];
  /** Prompt templates (loaded from .piko/prompts/). */
  promptTemplates?: PromptTemplate[];
  /** Settings manager for layered configuration (compaction, model defaults, etc.). */
  settingsManager?: SettingsManager;
  /** Skip loading AGENTS.md / CLAUDE.md context files. */
  skipContextFiles?: boolean;
  /** Custom tools registered by extensions. */
  customTools?: Array<{
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
    executor: (args: Record<string, unknown>) => Promise<unknown> | unknown;
  }>;
}

export interface StreamPromptOptions {
  settingsOverride?: Partial<EngineRunSettings>;
}

// ---- Results ----

export interface StreamPromptResult {
  messages: Message[];
  appendedMessages: Message[];
  status: HostRunResult["status"];
  sessionId: string;
  sessionFile?: string;
}

export interface HostRunResult {
  messages: Message[];
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps" | "context_overflow";
  sessionId: string;
  sessionFile?: string;
}

// ---- Host ----

export class PikoHost {
  private engine: StatelessEngine;
  private config: HostConfig;
  private approvalHandler?: ApprovalHandler;
  private systemPrompt: string;
  private sessionRuntime: PikoSessionRuntime;
  private settingsManager?: SettingsManager;

  // ---- Runtime mutable state (P0: Runtime Model Switching) ----
  private _thinkingLevel: string = "off";

  // ---- Agent loop queues (P1: Agent Loop Semantics) ----
  private steeringQueue: SteeringMessage[] = [];
  private followUpQueue: FollowUpMessage[] = [];
  private nextTurnQueue: NextTurnMessage[] = [];

  // ---- Skills & templates (P2: Host capabilities) ----
  private _skills: ReturnType<typeof loadSkills>["skills"] = [];
  private _promptTemplates: PromptTemplate[] = [];

  private constructor(
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
    this.systemPrompt =
      options.systemPrompt ??
      this.buildEnhancedSystemPrompt(
        cwd,
        options.appendSystemPrompt,
        options.promptGuidelines,
        options.promptTemplates,
        options.skipContextFiles,
      );
    this.sessionRuntime = sessionRuntime;

    // Load skills and templates for runtime invocation
    const skillsResult = loadSkills({ cwd });
    this._skills = skillsResult.skills;
    this._promptTemplates = options.promptTemplates ?? loadPromptTemplates({ cwd });
  }

  private buildEnhancedSystemPrompt(
    cwd: string,
    appendSystemPrompt?: string,
    promptGuidelines?: string[],
    promptTemplates?: PromptTemplate[],
    skipContextFiles?: boolean,
  ): string {
    const tools = this.engine.capabilities.tools.map((t) => ({
      name: t.name,
      snippet: t.description,
    }));
    const toolSnippets: Record<string, string> = {};
    for (const t of tools) toolSnippets[t.name] = t.snippet;

    const contextFiles = skipContextFiles ? [] : loadContextFiles({ cwd });
    const skills = loadSkills({ cwd });

    // Load prompt templates if not explicitly provided
    const templates = promptTemplates ?? loadPromptTemplates({ cwd });

    return buildSystemPrompt({
      cwd,
      selectedTools: tools.map((t) => t.name),
      toolSnippets,
      contextFiles,
      skills: skills.skills,
      promptGuidelines,
      appendSystemPrompt,
      promptTemplates: templates.length > 0 ? templates : undefined,
    });
  }

  // ---- P0: Runtime Model Switching ----

  /** Update the full host configuration at runtime (model, provider, settings). */
  setConfig(config: HostConfig): void {
    this.config = config;
  }

  /** Get the current host configuration. */
  getConfig(): HostConfig {
    return this.config;
  }

  /** Set thinking level. */
  setThinkingLevel(level: string): void {
    this._thinkingLevel = level;
  }

  /** Get current thinking level. */
  getThinkingLevel(): string {
    return this._thinkingLevel;
  }

  // ---- P1: Agent Loop APIs ----

  /** Queue a message to be injected at the next turn start (steering while streaming). */
  steer(text: string): void {
    this.steeringQueue.push({ text });
  }

  /** Queue a follow-up message that triggers another turn after current one completes. */
  followUp(text: string): void {
    this.followUpQueue.push({ text });
  }

  /** Queue a message for the next full turn after the agent finishes. */
  nextTurn(text: string): void {
    this.nextTurnQueue.push({ text });
  }

  // ---- P2: Skills & Prompt Templates ----

  /**
   * Invoke a skill by name, adding its content and optional additional instructions
   * as a user message and running the agent.
   */
  async runSkill(
    name: string,
    additionalInstructions?: string,
    signal?: AbortSignal,
  ): Promise<HostRunResult> {
    const skill = this._skills.find((s) => s.name === name);
    if (!skill) {
      throw new Error(`Unknown skill: ${name}`);
    }
    const prompt = formatSkillPrompt(skill, additionalInstructions);
    return this.run(prompt, signal);
  }

  /**
   * Streaming variant of runSkill. Formats the skill prompt and streams the response.
   */
  streamSkill(
    name: string,
    additionalInstructions?: string,
    signal?: AbortSignal,
  ): EventStream<EngineEvent, StreamPromptResult> {
    const skill = this._skills.find((s) => s.name === name);
    if (!skill) {
      const stream = new EventStreamImpl<EngineEvent, StreamPromptResult>();
      stream.push({ type: "error", message: `Unknown skill: ${name}` });
      stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      return stream;
    }
    const prompt = formatSkillPrompt(skill, additionalInstructions);
    return this.streamPrompt(prompt, {}, signal);
  }

  /**
   * Invoke a prompt template by name, expanding it with arguments and running the agent.
   */
  async runPromptTemplate(
    name: string,
    args: string[] = [],
    signal?: AbortSignal,
  ): Promise<HostRunResult> {
    const template = this._promptTemplates.find((t) => t.name === name);
    if (!template) {
      throw new Error(`Unknown prompt template: ${name}`);
    }
    const expanded = substituteArgs(template.content, args);
    return this.run(`Run template /${name}: ${expanded}`, signal);
  }

  /**
   * Streaming variant of runPromptTemplate. Expands template args and streams the response.
   */
  streamPromptTemplate(
    name: string,
    args: string[] = [],
    signal?: AbortSignal,
  ): EventStream<EngineEvent, StreamPromptResult> {
    const template = this._promptTemplates.find((t) => t.name === name);
    if (!template) {
      const stream = new EventStreamImpl<EngineEvent, StreamPromptResult>();
      stream.push({ type: "error", message: `Unknown prompt template: ${name}` });
      stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      return stream;
    }
    const expanded = substituteArgs(template.content, args);
    return this.streamPrompt(`Run template /${name}: ${expanded}`, {}, signal);
  }

  /** Get loaded skills (for TUI introspection). */
  get skills(): ReturnType<typeof loadSkills>["skills"] {
    return this._skills;
  }

  /** Get loaded prompt templates (for TUI introspection). */
  get promptTemplates(): PromptTemplate[] {
    return this._promptTemplates;
  }

  /** Build the prepareTurn callback for runScheduler based on current config. */
  private buildPrepareTurn(): () => TurnPreparation {
    return () => {
      const cfg = this.config;
      return {
        // Pass full model/provider objects so engine gets updated API keys, headers, context window etc. (fix #1)
        model: cfg.model,
        provider: cfg.provider,
        thinkingLevel: this._thinkingLevel !== "off" ? this._thinkingLevel : undefined,
        // Do NOT set settingsOverride — base settings come from config passed to runScheduler.
        // Setting it here would overwrite per-call overrides from streamPrompt(). (fix #4)
      };
    };
  }

  /** Get the effective compaction settings (from SettingsManager or defaults). */
  private getCompactionSettings(): CompactionSettings {
    if (this.settingsManager) {
      const s = this.settingsManager.getCompactionSettings();
      return {
        enabled: s.enabled,
        reserveTokens: s.reserveTokens,
        keepRecentTokens: s.keepRecentTokens,
      };
    }
    return { enabled: true, reserveTokens: 16384, keepRecentTokens: 20000 };
  }

  /** Run LLM-based compaction on the current session. Follows pi's pattern. */
  async compact(_customInstructions?: string): Promise<void> {
    const s = this.getCompactionSettings();
    const entries = await this.sessionManager.getBranch();
    const prep = prepareCompaction(entries, s);
    if (!prep.ok || !prep.value) return;
    const apiKey = this.config.provider.apiKey ?? "";
    const cr = await compact(prep.value, this.config.model as any, apiKey);
    if (!cr.ok) return;
    await this.sessionManager.appendCompaction(
      cr.value.summary,
      cr.value.firstKeptEntryId,
      cr.value.tokensBefore,
      cr.value.details,
    );
  }

  /** Fire-and-forget threshold compaction check. */
  async maybeCompact(): Promise<void> {
    const s = this.getCompactionSettings();
    if (!s.enabled) return;
    const msgs = await this.sessionManager.loadMessages();
    const ctxTokens = estimateContextTokens(msgs as any).tokens;
    const cw = (this.config.model as { contextWindow?: number }).contextWindow ?? 200_000;
    if (shouldCompact(ctxTokens, cw, s)) await this.compact();
  }

  /** Convenience: engine's tool info list. */
  get availableTools(): EngineToolInfo[] {
    return this.engine.capabilities.tools;
  }

  // ---- Factories ----

  static async create(options: PikoHostCreateOptions): Promise<PikoHost> {
    const sessionRuntime = await PikoSessionRuntime.create(options.session);

    // Build custom tool definitions and registry from options
    const customToolDefs: EngineTool[] | undefined = options.customTools?.map((t) => ({
      name: t.name,
      description: t.description,
      inputSchema: t.inputSchema as EngineTool["inputSchema"],
      executor: { kind: "native" as const, target: t.name },
    }));
    const customToolRegistry:
      | Record<string, (args: Record<string, unknown>) => Promise<unknown>>
      | undefined = options.customTools?.reduce(
      (acc, t) => {
        acc[t.name] = (args: Record<string, unknown>) => Promise.resolve(t.executor(args));
        return acc;
      },
      {} as Record<string, (args: Record<string, unknown>) => Promise<unknown>>,
    );

    const engine =
      options.engine ??
      createNativeEngine({
        cwd: sessionRuntime.getCwd(),
        toolRegistry: customToolRegistry,
        toolDefinitions: customToolDefs,
      });
    return new PikoHost(engine, options.config, sessionRuntime, {
      approvalHandler: options.approvalHandler,
      systemPrompt: options.systemPrompt,
      appendSystemPrompt: options.appendSystemPrompt,
      promptGuidelines: options.promptGuidelines,
      promptTemplates: options.promptTemplates,
      settingsManager: options.settingsManager,
      skipContextFiles: options.skipContextFiles,
    });
  }

  /** Test / migration helper: create a host wrapping an existing SessionManager. */
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
    return new PikoHost(engine, config, sessionRuntime, {
      approvalHandler: options.approvalHandler,
      systemPrompt: options.systemPrompt,
      settingsManager: options.settingsManager,
    });
  }

  // ---- Session accessors ----

  /** Access the settings manager (if configured). */
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

  // ---- Session metadata / management ----

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
    return options.namedOnly ? sessions.filter((session) => Boolean(session.name)) : sessions;
  }

  async renameSession(specifier: string, name?: string): Promise<boolean> {
    return SessionManager.rename(specifier, name, this.cwd);
  }

  async deleteSession(specifier: string): Promise<boolean> {
    return SessionManager.delete(specifier, this.cwd);
  }

  async branchToEntry(entryId: string): Promise<void> {
    // Auto-summarize before branching; pass summary to navigation via branchWithSummary (fix #3)
    const summary = await this.autoBranchSummary();
    if (summary) {
      await this.sessionManager.branchWithSummary(entryId, summary);
    } else {
      await this.sessionManager.branch(entryId);
    }
  }

  async branchToEntryWithSummary(entryId: string, summary: string): Promise<void> {
    await this.sessionManager.branchWithSummary(entryId, summary);
  }

  /** Generate a branch summary for the current branch before navigation. Returns the summary text or undefined. */
  private async autoBranchSummary(): Promise<string | undefined> {
    const bsSettings = this.settingsManager?.getBranchSummarySettings?.() ?? {
      reserveTokens: 16384,
      skipPrompt: false,
    };
    if (bsSettings.skipPrompt) return undefined;

    try {
      const entries = await this.sessionManager.getBranch();
      if (entries.length === 0) return undefined;

      const apiKey = this.config.provider.apiKey ?? "";
      if (!apiKey) return undefined;

      const branchSummary = await generateBranchSummary(entries, {
        model: this.config.model as any,
        apiKey,
        signal: new AbortController().signal,
        reserveTokens: bsSettings.reserveTokens,
      });

      if (branchSummary.ok) {
        const msg = branchSummary.value.summary;
        if (msg && msg !== "No content to summarize") {
          return msg;
        }
      }
    } catch {
      // Non-fatal: branch summary failure shouldn't block navigation
    }
    return undefined;
  }

  /** Get messages on the divergent path from oldLeafId to newLeafId */
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

  // ---- Session lifecycle ----

  get diagnostics(): readonly import("./session/session-runtime.js").SessionRuntimeDiagnostic[] {
    return this.sessionRuntime.diagnostics;
  }

  onSessionReplaced(handler: (event: ReplaceSessionEvent) => Promise<void> | void): void {
    this.sessionRuntime.setOnSessionReplaced(handler);
  }

  /** Synchronous hook called after teardown, before new session is applied. */
  onBeforeInvalidate(handler: () => void): void {
    this.sessionRuntime.setOnBeforeInvalidate(handler);
  }

  /** Called after the new session is applied and onSessionReplaced fires. */
  onAfterRebind(handler: () => Promise<void> | void): void {
    this.sessionRuntime.setOnAfterRebind(handler);
  }

  async switchSession(specifier: string): Promise<SessionManager | null> {
    return this.sessionRuntime.switchSession(specifier);
  }

  async newSession(options: { parentSession?: string } = {}): Promise<SessionManager> {
    return this.sessionRuntime.newSession(options);
  }

  async cloneSession(): Promise<SessionManager> {
    return this.sessionRuntime.cloneSession();
  }

  async forkSession(
    entryId: string,
    options?: Parameters<SessionManager["fork"]>[1],
  ): Promise<Awaited<ReturnType<SessionManager["fork"]>>> {
    return this.sessionRuntime.forkSession(entryId, options);
  }

  async importSession(inputPath: string): Promise<SessionManager> {
    return this.sessionRuntime.importFromJsonl(inputPath);
  }

  async dispose(): Promise<void> {
    await this.sessionRuntime.dispose();
  }

  private async loadSessionState(): Promise<ReturnType<typeof createSession>> {
    const existingMessages = await this.sessionManager.loadMessages();
    return createSession({
      sessionId: this.sessionManager.getSessionId(),
      messages: existingMessages,
      systemPrompt: this.systemPrompt,
    });
  }

  /** Build retry config from SettingsManager. */
  private getRetryConfig(): { maxRetries: number; baseDelayMs: number } | undefined {
    if (this.settingsManager) {
      const r = this.settingsManager.getRetrySettings();
      if (r.enabled) return { maxRetries: r.maxRetries, baseDelayMs: r.baseDelayMs };
      return undefined;
    }
    // Default: 1 retry with 2s base delay
    return { maxRetries: 1, baseDelayMs: 2000 };
  }

  // ---- Run (multi-step, non-streaming) ----

  async run(prompt: string, signal?: AbortSignal): Promise<HostRunResult> {
    const loadedSession = await this.loadSessionState();
    const session = addUserMessage(loadedSession, prompt);

    // Reset per-run queues
    this.steeringQueue = [];
    this.followUpQueue = [];
    this.nextTurnQueue = [];

    const result = await runScheduler({
      engine: this.engine,
      config: this.config,
      session,
      approvalHandler: this.approvalHandler,
      signal,
      retry: this.getRetryConfig(),
      prepareTurn: this.buildPrepareTurn(),
      steeringQueue: this.steeringQueue,
      followUpQueue: this.followUpQueue,
      nextTurnQueue: this.nextTurnQueue,
    });

    await this.sessionManager.saveMessages(this.config.model.id, result.session.messages);
    this.maybeCompact().catch(() => {});

    return {
      messages: result.session.messages,
      totalSteps: result.totalSteps,
      status: result.status,
      sessionId: this.sessionId,
      sessionFile: this.sessionFile,
    };
  }

  // ---- Stream prompt (multi-step, streaming) ----

  streamPrompt(
    prompt: string,
    options: StreamPromptOptions = {},
    signal?: AbortSignal,
  ): EventStream<EngineEvent, StreamPromptResult> {
    const stream = new EventStreamImpl<EngineEvent, StreamPromptResult>();

    // Reset per-run queues
    this.steeringQueue = [];
    this.followUpQueue = [];
    this.nextTurnQueue = [];

    void this.loadSessionState()
      .then(async (session) => {
        const nextSession = addUserMessage(session, prompt);
        const result = await runScheduler({
          engine: this.engine,
          config: {
            ...this.config,
            settings: {
              ...this.config.settings,
              ...options.settingsOverride,
            },
          },
          session: nextSession,
          approvalHandler: this.approvalHandler,
          signal,
          retry: this.getRetryConfig(),
          prepareTurn: this.buildPrepareTurn(),
          steeringQueue: this.steeringQueue,
          followUpQueue: this.followUpQueue,
          nextTurnQueue: this.nextTurnQueue,
          onEvent: (event) => {
            stream.push(event);
          },
        });

        await this.sessionManager.saveMessages(this.config.model.id, result.session.messages);
        this.maybeCompact().catch(() => {});

        const appendedMessages = result.session.messages.slice(nextSession.messages.length);
        stream.end({
          messages: result.session.messages,
          appendedMessages,
          status: result.status,
          sessionId: this.sessionId,
          sessionFile: this.sessionFile,
        });
      })
      .catch((err) => {
        const message = err instanceof Error ? err.message : String(err);
        stream.push({ type: "error", message });
        stream.end({
          messages: [],
          appendedMessages: [],
          status: "error",
          sessionId: "",
        });
      });

    return stream;
  }
}

// ---- P2 helper: format skill as prompt ----

/** Format a skill as a prompt string. Exported for TUI use (fix #3). */
export function formatSkillPrompt(
  skill: { name: string; filePath: string; description: string },
  additionalInstructions?: string,
): string {
  let prompt = `Read and follow the skill at @${skill.filePath}: ${skill.description}`;
  if (additionalInstructions) {
    prompt += `\n\nAdditional instructions: ${additionalInstructions}`;
  }
  return prompt;
}
