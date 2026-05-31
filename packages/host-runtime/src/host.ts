import { createNativeEngine } from "piko-engine-native";
import type {
  EngineEvent,
  EngineRunSettings,
  EngineToolInfo,
  EventStream,
  Message,
  StatelessEngine,
} from "piko-engine-protocol";
import { EventStream as EventStreamImpl } from "piko-engine-protocol";
import type { ApprovalHandler } from "./approval-controller.js";
import {
  loadContextFiles,
  loadPromptTemplates,
  type PromptTemplate,
} from "./prompts/index.js";
import type { HostConfig } from "./models/index.js";
import { runScheduler } from "./scheduler.js";
import type { SettingsManager } from "./settings/index.js";
import type { Session, SessionMeta } from "./session/index.js";
import {
  compact,
  estimateContextTokens,
  prepareCompaction,
  shouldCompact,
  type CompactionSettings,
} from "./compaction/index.js";
import {
  addUserMessage,
  type CreateSessionRuntimeOptions,
  createSession,
  JsonlSessionRepo,
  NodeExecutionEnv,
  PikoSessionRuntime,
  type ReplaceSessionEvent,
  SessionManager,
  getSessionDir,
} from "./session/index.js";
import { loadSkills } from "./skills/index.js";
import { buildSystemPrompt } from "./prompts/index.js";

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
    } = {},
  ) {
    this.engine = engine;
    this.config = config;
    this.approvalHandler = options.approvalHandler;
    this.settingsManager = options.settingsManager;
    const cwd = sessionRuntime.getCwd();
    this.systemPrompt = options.systemPrompt
      ?? this.buildEnhancedSystemPrompt(cwd, options.appendSystemPrompt, options.promptGuidelines, options.promptTemplates);
    this.sessionRuntime = sessionRuntime;
  }

  private buildEnhancedSystemPrompt(
    cwd: string,
    appendSystemPrompt?: string,
    promptGuidelines?: string[],
    promptTemplates?: PromptTemplate[],
  ): string {
    const tools = this.engine.capabilities.tools.map((t) => ({ name: t.name, snippet: t.description }));
    const toolSnippets: Record<string, string> = {};
    for (const t of tools) toolSnippets[t.name] = t.snippet;

    const contextFiles = loadContextFiles({ cwd });
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

  /** Get the effective compaction settings (from SettingsManager or defaults). */
  private getCompactionSettings(): CompactionSettings {
    if (this.settingsManager) {
      const s = this.settingsManager.getCompactionSettings();
      return { enabled: s.enabled, reserveTokens: s.reserveTokens, keepRecentTokens: s.keepRecentTokens };
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
      cr.value.summary, cr.value.firstKeptEntryId, cr.value.tokensBefore, cr.value.details,
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
    const engine =
      options.engine ??
      createNativeEngine({
        cwd: sessionRuntime.getCwd(),
      });
    return new PikoHost(engine, options.config, sessionRuntime, {
      approvalHandler: options.approvalHandler,
      systemPrompt: options.systemPrompt,
      appendSystemPrompt: options.appendSystemPrompt,
      promptGuidelines: options.promptGuidelines,
      promptTemplates: options.promptTemplates,
      settingsManager: options.settingsManager,
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
    // Auto-summarize before branching if compaction is enabled
    await this.autoBranchSummary();
    await this.sessionManager.branch(entryId);
  }

  async branchToEntryWithSummary(entryId: string, summary: string): Promise<void> {
    await this.sessionManager.branchWithSummary(entryId, summary);
  }

  /** Trigger branch summary compaction before navigation when enabled. */
  private async autoBranchSummary(): Promise<void> {
    const s = this.getCompactionSettings();
    if (!s.enabled) return;
    try {
      await this.compact();
    } catch {
      // Non-fatal: compaction failure shouldn't block navigation
    }
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

    const result = await runScheduler({
      engine: this.engine,
      config: this.config,
      session,
      approvalHandler: this.approvalHandler,
      signal,
      retry: this.getRetryConfig(),
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
