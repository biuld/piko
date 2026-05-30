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
import type { SessionMeta } from "./file-session-store.js";
import type { HostConfig } from "./model-config.js";
import { runScheduler } from "./scheduler.js";
import { SessionManager } from "./session-manager.js";
import {
  type CreateSessionRuntimeOptions,
  PikoSessionRuntime,
  type ReplaceSessionEvent,
} from "./session-runtime.js";
import { addUserMessage, createSession } from "./session-store.js";

// ---- Options ----

export interface PikoHostCreateOptions {
  /** Engine implementation. Defaults to native engine with pi-ai LLM caller. */
  engine?: StatelessEngine;
  config: HostConfig;
  approvalHandler?: ApprovalHandler;
  systemPrompt?: string;
  session?: CreateSessionRuntimeOptions;
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
  status: "completed" | "aborted" | "error" | "max_steps";
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

  private constructor(
    engine: StatelessEngine,
    config: HostConfig,
    sessionRuntime: PikoSessionRuntime,
    options: {
      approvalHandler?: ApprovalHandler;
      systemPrompt?: string;
    } = {},
  ) {
    this.engine = engine;
    this.config = config;
    this.approvalHandler = options.approvalHandler;
    this.systemPrompt = options.systemPrompt ?? this.buildDefaultSystemPrompt();
    this.sessionRuntime = sessionRuntime;
  }

  private buildDefaultSystemPrompt(): string {
    const tools = this.engine.capabilities.tools;
    const lines: string[] = ["You are a helpful assistant. Be concise."];
    if (tools.length > 0) {
      lines.push("", "Available tools:", ...tools.map((t) => `- ${t.name}: ${t.description}`));
    }
    return lines.join("\n");
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
    } = {},
  ): PikoHost {
    const sessionRuntime = PikoSessionRuntime.fromSessionManager(sessionManager);
    return new PikoHost(engine, config, sessionRuntime, options);
  }

  // ---- Session accessors ----

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
    await this.sessionManager.branch(entryId);
  }

  async getBranchEntries(): Promise<Awaited<ReturnType<SessionManager["getBranch"]>>> {
    return this.sessionManager.getBranch();
  }

  async getTreeEntries(): Promise<Awaited<ReturnType<SessionManager["getTree"]>>> {
    return this.sessionManager.getTree();
  }

  // ---- Session lifecycle ----

  get diagnostics(): readonly import("./session-runtime.js").SessionRuntimeDiagnostic[] {
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

  // ---- Internal: load in-memory session state from the current SessionManager ----

  private async loadSessionState(): Promise<ReturnType<typeof createSession>> {
    const existingMessages = await this.sessionManager.loadMessages();
    return createSession({
      sessionId: this.sessionManager.getSessionId(),
      messages: existingMessages,
      systemPrompt: this.systemPrompt,
    });
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
    });

    await this.sessionManager.saveMessages(this.config.model.id, result.session.messages);

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
          onEvent: (event) => {
            stream.push(event);
          },
        });

        await this.sessionManager.saveMessages(this.config.model.id, result.session.messages);

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
