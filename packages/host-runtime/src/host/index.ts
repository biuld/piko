import type { ModelStepEvent } from "../models/executor.js";
import type { HostConfig, ModelRegistry } from "../models/index.js";
import { EventBus, OrchdRpcClient } from "../orchd/index.js";
import {
  EventStream,
  type Message,
  type Orchestrator,
  type HostEvent,
} from "../orchd/protocol/index.js";
import { type ContextFile, loadContextFiles, type PromptTemplate } from "../prompts/index.js";
import type { SessionPersistenceOverview, TreeNavigationResult } from "../session/index.js";
import {
  PikoSessionRuntime,
  type ReplaceSessionEvent,
  type SessionManager,
} from "../session/index.js";
import { SettingsManager } from "../settings/index.js";
import type { Skill } from "../skills/index.js";
import type { McpServerManager } from "../tools/mcp-provider.js";
import { HostPersistence } from "./persistence/index.js";
import { HostQueueController, type PromptBehavior } from "./queue/index.js";
import { HostResourcesController } from "./resources/index.js";
import { AgentNameAssigner, builtinToolSet, HostRunController } from "./run/index.js";
import { HostRuntimeConfigController } from "./runtime-config/index.js";
import { type CompactResult, HostSessionController } from "./session/index.js";
import type {
  FollowUpMessage,
  HostRunResult,
  HostToolCallbacks,
  NextTurnMessage,
  PikoHostCreateOptions,
  QueueMode,
  SteeringMessage,
  StreamPromptOptions,
  StreamPromptResult,
  ToolApprovalHandler,
} from "./shared/index.js";
import { HostState } from "./state/index.js";

export type { PromptBehavior } from "./queue/index.js";
export { formatSkillPrompt } from "./resources/index.js";
// ---- Types (re-exported) ----
export type { CompactResult } from "./session/index.js";
export type {
  HostRunResult,
  HostToolCallbacks,
  PikoHostCreateOptions,
  StreamPromptOptions,
  StreamPromptResult,
} from "./shared/index.js";
export { HostState } from "./state/index.js";

// ---- Host ----

export class PikoHost {
  readonly version = "0.1.0";
  private systemPrompt: string;
  private settingsManager: SettingsManager;
  private _orchestrator?: Orchestrator;
  private state = new HostState();
  private mcpManager?: McpServerManager;
  private persistence: HostPersistence;
  private sessionController: HostSessionController;
  private runtimeConfig: HostRuntimeConfigController;
  private queueController: HostQueueController;
  private resourcesController: HostResourcesController;
  private runController: HostRunController;
  private agentNameAssigner: AgentNameAssigner;
  private modelRegistry?: ModelRegistry;
  /** Central event bus for unified HostEvent publish/subscribe. */
  public readonly eventBus = new EventBus();
  public debugTracePath?: string;

  private get config(): HostConfig {
    return this.runtimeConfig.getConfig();
  }

  /**
   * Throw if a run is in progress. Session mutations (branch, fork, switch)
   * require an idle harness to avoid conflicts with in-progress writes.
   */
  private _ensureIdle(): void {
    if (this._orchestrator) {
      const state = this._orchestrator.snapshot();
      const anyRunning = Object.values(state.agents).some((a) => a.status === "running");
      if (anyRunning) {
        throw new Error("Cannot perform session mutation while a run is in progress");
      }
    }
  }

  constructor(
    config: HostConfig,
    sessionRuntime: PikoSessionRuntime,
    options: {
      approvalHandler?: ToolApprovalHandler;
      hostToolCallbacks?: HostToolCallbacks;
      systemPrompt?: string;
      appendSystemPrompt?: string;
      promptGuidelines?: string[];
      promptTemplates?: PromptTemplate[];
      skills?: Skill[];
      settingsManager: SettingsManager;
      skipContextFiles?: boolean;
      orchestrator?: Orchestrator;
      modelRegistry?: ModelRegistry;
    },
  ) {
    this._orchestrator = options.orchestrator;
    this.persistence = new HostPersistence(
      () => this.sessionManager,
      () => this.config.model.id,
    );
    this.sessionController = new HostSessionController(
      sessionRuntime,
      this.persistence,
      this.state,
      () => this._ensureIdle(),
      () => this.config,
      () => this.settingsManager,
      () => this._orchestrator,
    );
    this.sessionController.refreshPersistenceOverview().catch(() => {});
    this.runtimeConfig = new HostRuntimeConfigController(
      config,
      () => this.sessionManager,
      this.state,
      () => this.sessionController.refreshPersistenceOverview(),
      options.modelRegistry,
      options.settingsManager.getDefaultThinkingLevel(),
    );
    this.modelRegistry = options.modelRegistry;
    this.queueController = new HostQueueController(
      this.state,
      (agentId) => this.isRunning(agentId),
      () => this.sessionId,
      (text, streamOptions, signal) => this.streamPromptLifecycle(text, streamOptions, signal),
    );

    this.settingsManager = options.settingsManager;
    this.agentNameAssigner = new AgentNameAssigner(this.settingsManager.getAgentNames());

    this.runController = new HostRunController({
      getOrchestrator: () => this._orchestrator,
      getConfig: () => this.config,
      getSystemPrompt: () => this.systemPrompt,
      getActiveToolNames: () => this.getActiveToolNames(),
      getMcpServers: () => this.settingsManager.settings.mcpServers,
      getMcpManager: () => this.mcpManager,
      setMcpManager: (manager) => {
        this.mcpManager = manager;
      },
      getSessionManager: () => this.sessionManager,
      agentNameAssigner: this.agentNameAssigner,
      persistence: this.persistence,
      state: this.state,
      eventBus: this.eventBus,
    });

    this.settingsManager.onChange((settings) => {
      if (settings.agentNames) {
        this.agentNameAssigner.setNames(settings.agentNames);
      }
      if (settings.defaultThinkingLevel !== undefined) {
        this.setThinkingLevel(settings.defaultThinkingLevel);
      }
    });
    this.systemPrompt = options.systemPrompt ?? "";
    this.resourcesController = new HostResourcesController({
      skills: options.skills ?? [],
      promptTemplates: options.promptTemplates,
      runtimeConfig: this.runtimeConfig,
      run: (resourcePrompt, signal) => this.run(resourcePrompt, signal),
      streamPrompt: (resourcePrompt, signal) => this.streamPrompt(resourcePrompt, {}, signal),
    });
  }

  // ---- P0: Runtime Model Switching ----

  setConfig(config: HostConfig): void {
    this.runtimeConfig.setConfig(config);
  }

  getConfig(): HostConfig {
    return this.runtimeConfig.getConfig();
  }

  setThinkingLevel(level: string): void {
    this.runtimeConfig.setThinkingLevel(level);
  }

  getThinkingLevel(): string {
    return this.runtimeConfig.getThinkingLevel();
  }

  /** Set steering mode: consume all queued steering at once or one at a time. */
  setSteeringMode(_mode: QueueMode): void {
    this.queueController.setSteeringMode(_mode);
  }

  /** Set follow-up mode: consume all queued follow-ups at once or one at a time. */
  setFollowUpMode(_mode: QueueMode): void {
    this.queueController.setFollowUpMode(_mode);
  }

  getActiveToolNames(): string[] | undefined {
    return this.runtimeConfig.getActiveToolNames();
  }

  /** Total count of all registered tools (builtin only). */
  getTotalToolCount(): number {
    return builtinToolSet.tools.length;
  }

  setActiveToolNames(toolNames: string[] | undefined): void {
    this.runtimeConfig.setActiveToolNames(toolNames);
  }

  // ---- Lifecycle callback (persistent, registered by TUI) ----

  /**
   * Register a persistent lifecycle event callback.
   * The TUI registers this once; the Host emits queue_update etc. through it.
   */
  setLifecycleCallback(cb: (event: HostEvent) => void): void {
    this.queueController.setLifecycleCallback(cb);
  }

  // ---- P1: Agent Loop APIs ----

  /**
   * Queue a steering message to inject during the current run.
   * Rejects if no run is in progress.
   * Emits queue_update immediately so the TUI can reflect the change.
   */
  steer(
    text: string,
    images?: Parameters<HostQueueController["steer"]>[1],
    agentId = "main",
  ): void {
    this.queueController.steer(text, images, agentId);
  }

  /**
   * Queue a follow-up message to run after the current turn completes.
   * Rejects if no run is in progress.
   * Emits queue_update immediately so the TUI can reflect the change.
   */
  followUp(
    text: string,
    images?: Parameters<HostQueueController["followUp"]>[1],
    agentId = "main",
  ): void {
    this.queueController.followUp(text, images, agentId);
  }

  /**
   * Queue a message for the next full turn. Can be called anytime.
   * Emits queue_update immediately.
   */
  nextTurn(
    text: string,
    images?: Parameters<HostQueueController["nextTurn"]>[1],
    agentId = "main",
  ): void {
    this.queueController.nextTurn(text, images, agentId);
  }

  // ---- P1b: Unified prompt entry ----

  /**
   * Unified prompt entry — the Host routes based on current phase.
   *
   *   idle → starts a new stream (streamPrompt)
   *   running + behavior === "auto" | "steer" → queues as steering
   *   running + behavior === "followUp" → queues as follow-up
   *
   * Returns null when the message was queued (not streamed yet).
   */
  prompt(
    text: string,
    behavior: PromptBehavior = "auto",
    agentId = "main",
    signal?: AbortSignal,
  ): EventStream<HostEvent, StreamPromptResult> | null {
    return this.queueController.prompt(text, behavior, agentId, signal);
  }

  /** Is a run currently in progress? Checks the orchestrator agent's status. */
  isRunning(agentId = "main"): boolean {
    if (!this._orchestrator) return false;
    return this._orchestrator.snapshot().agents[agentId]?.status === "running";
  }

  // ---- P1c: Queue introspection & dequeue ----

  /** Read-only snapshot of all queues. */
  getQueueState(agentId = "main"): {
    steering: ReadonlyArray<SteeringMessage>;
    followUp: ReadonlyArray<FollowUpMessage>;
    nextTurn: ReadonlyArray<NextTurnMessage>;
  } {
    return this.queueController.getQueueState(agentId);
  }

  /**
   * Clear all queues and return the drained messages.
   * Emits queue_update (empty) so the TUI hides the queue display.
   */
  dequeue(agentId = "main"): {
    steering: SteeringMessage[];
    followUp: FollowUpMessage[];
    nextTurn: NextTurnMessage[];
  } {
    return this.queueController.dequeue(agentId);
  }

  // ---- P2: Skills & Templates ----

  async runSkill(
    name: string,
    additionalInstructions?: string,
    signal?: AbortSignal,
  ): Promise<HostRunResult> {
    return this.resourcesController.runSkill(name, additionalInstructions, signal);
  }

  streamSkill(
    name: string,
    additionalInstructions?: string,
    signal?: AbortSignal,
  ): EventStream<ModelStepEvent, StreamPromptResult> {
    return this.resourcesController.streamSkill(name, additionalInstructions, signal);
  }

  async runPromptTemplate(
    name: string,
    args: string[] = [],
    signal?: AbortSignal,
  ): Promise<HostRunResult> {
    return this.resourcesController.runPromptTemplate(name, args, signal);
  }

  streamPromptTemplate(
    name: string,
    args: string[] = [],
    signal?: AbortSignal,
  ): EventStream<ModelStepEvent, StreamPromptResult> {
    return this.resourcesController.streamPromptTemplate(name, args, signal);
  }

  get skills(): HostResourcesController["skills"] {
    return this.resourcesController.skills;
  }
  get promptTemplates(): PromptTemplate[] {
    return this.resourcesController.promptTemplates;
  }
  async getContextFiles(): Promise<ContextFile[]> {
    return loadContextFiles({ cwd: this.cwd });
  }

  // ---- Session state restoration ----

  async restoreFromSession(): Promise<void> {
    await this.runtimeConfig.restoreFromSession();
  }

  // ---- Compaction & Branch Summary ----

  getCompactionSettings() {
    return this.sessionController.getCompactionSettings();
  }

  async compact(customInstructions?: string): Promise<CompactResult> {
    return this.sessionController.compact(customInstructions);
  }

  async maybeCompact(): Promise<CompactResult> {
    return this.sessionController.maybeCompact();
  }

  async navigateToEntry(entryId: string): Promise<TreeNavigationResult> {
    return this.sessionController.navigateToEntry(entryId);
  }

  async branchToEntry(entryId: string): Promise<void> {
    return this.sessionController.branchToEntry(entryId);
  }

  async branchToEntryWithSummary(entryId: string, summary: string): Promise<void> {
    return this.sessionController.branchToEntryWithSummary(entryId, summary);
  }

  // ---- Orchestrator access -------

  /** The orchestrator, if multi-agent mode is enabled. */
  get orchestrator(): Orchestrator | undefined {
    return this._orchestrator;
  }

  /** Whether multi-agent team mode is enabled. */
  get teamMode(): boolean {
    return this._orchestrator !== undefined;
  }

  /** Get the orchestrator graph snapshot for TUI rendering. */
  getOrchestratorGraph() {
    return { nodes: [], edges: [] };
  }

  /** Get the orchestrator state snapshot. */
  getOrchestratorSnapshot() {
    return this._orchestrator?.snapshot();
  }

  // ---- Factories (static) ----

  static async create(options: PikoHostCreateOptions): Promise<PikoHost> {
    const { createPikoHost } = await import("./factories.js");
    return createPikoHost(options);
  }

  static fromSessionManager(
    config: HostConfig,
    sessionManager: SessionManager,
    options?: {
      approvalHandler?: ToolApprovalHandler;
      hostToolCallbacks?: HostToolCallbacks;
      orchestrator?: Orchestrator;
      systemPrompt?: string;
      settingsManager?: SettingsManager;
    },
  ): PikoHost {
    const sessionRuntime = PikoSessionRuntime.fromSessionManager(sessionManager);
    const orchestrator =
      options?.orchestrator ?? new OrchdRpcClient({ cwd: sessionManager.getCwd() });
    const settingsManager = options?.settingsManager ?? SettingsManager.inMemory();
    return new PikoHost(config, sessionRuntime, {
      ...options,
      settingsManager,
      orchestrator,
    });
  }

  // ---- Session accessors ----

  getSettingsManager(): SettingsManager {
    return this.settingsManager;
  }
  get sessionManager(): SessionManager {
    return this.sessionController.sessionManager;
  }
  get sessionId(): string {
    return this.sessionController.sessionId;
  }
  get sessionFile(): string | undefined {
    return this.sessionController.sessionFile;
  }
  get cwd(): string {
    return this.sessionController.cwd;
  }

  async getSessionName(): Promise<string | undefined> {
    return this.sessionController.getSessionName();
  }
  async loadMessages(): Promise<Message[]> {
    return this.sessionController.loadMessages();
  }
  async loadBranchEntries(): ReturnType<SessionManager["loadBranchEntries"]> {
    return this.sessionController.loadBranchEntries();
  }
  getSessionPersistenceOverview(): SessionPersistenceOverview | undefined {
    return this.sessionController.getSessionPersistenceOverview();
  }
  async loadSessionPersistenceOverview(): Promise<SessionPersistenceOverview> {
    return this.sessionController.refreshPersistenceOverview();
  }
  async setSessionName(name?: string): Promise<void> {
    await this.sessionController.setSessionName(name);
  }
  isSessionPersisted(): boolean {
    return this.sessionController.isSessionPersisted();
  }
  getParentSessionPath(): string | undefined {
    return this.sessionController.getParentSessionPath();
  }
  getLeafId(): string | null {
    return this.sessionController.getLeafId();
  }

  async listSessions(
    options: { scope?: "current" | "all"; namedOnly?: boolean } = {},
  ): ReturnType<HostSessionController["listSessions"]> {
    return this.sessionController.listSessions(options);
  }

  async renameSession(specifier: string, name?: string): Promise<boolean> {
    return this.sessionController.renameSession(specifier, name);
  }
  async deleteSession(specifier: string): Promise<boolean> {
    return this.sessionController.deleteSession(specifier);
  }

  async getDivergentMessages(oldLeafId: string | null, newLeafId: string): Promise<number> {
    return this.sessionController.getDivergentMessages(oldLeafId, newLeafId);
  }

  async getBranchEntries(): Promise<Awaited<ReturnType<SessionManager["getBranch"]>>> {
    return this.sessionController.getBranchEntries();
  }
  async getTreeEntries(): Promise<Awaited<ReturnType<SessionManager["getTree"]>>> {
    return this.sessionController.getTreeEntries();
  }

  get diagnostics(): readonly import("../session/session-runtime.js").SessionRuntimeDiagnostic[] {
    return this.sessionController.diagnostics;
  }
  onSessionReplaced(handler: (event: ReplaceSessionEvent) => Promise<void> | void): void {
    this.sessionController.onSessionReplaced(handler);
  }
  onBeforeInvalidate(handler: () => void): void {
    this.sessionController.onBeforeInvalidate(handler);
  }
  onAfterRebind(handler: () => Promise<void> | void): void {
    this.sessionController.onAfterRebind(handler);
  }

  async switchSession(specifier: string): Promise<SessionManager | null> {
    return this.sessionController.switchSession(specifier);
  }
  async newSession(options: { parentSession?: string } = {}): Promise<SessionManager> {
    return this.sessionController.newSession(options);
  }
  async cloneSession(): Promise<SessionManager> {
    return this.sessionController.cloneSession();
  }

  async forkSession(
    entryId: string,
    options?: Parameters<SessionManager["fork"]>[1],
  ): Promise<Awaited<ReturnType<SessionManager["fork"]>>> {
    return this.sessionController.forkSession(entryId, options);
  }

  async importSession(inputPath: string): Promise<SessionManager> {
    return this.sessionController.importSession(inputPath);
  }
  async dispose(): Promise<void> {
    if (this.mcpManager) {
      await this.mcpManager.destroy();
      this.mcpManager = undefined;
    }
    await this.sessionController.dispose();
  }

  async refreshAuth(): Promise<void> {
    const config = this.getConfig();
    const provider = config.model.provider;
    const authStorage = this.modelRegistry?.getAuthStorage();
    if (authStorage) {
      const newKey = await authStorage.resolveOAuthApiKey(provider);
      if (newKey) {
        this.setConfig({
          ...config,
          provider: {
            ...config.provider,
            apiKey: newKey,
          },
        });
      }
    }
  }

  // ---- Run (multi-step, non-streaming) ----

  async run(prompt: string, signal?: AbortSignal, agentId = "main"): Promise<HostRunResult> {
    await this.refreshAuth();
    return await this.runController.run(prompt, signal, agentId);
  }

  // ---- Stream prompt (multi-step, streaming) ----

  streamPrompt(
    prompt: string,
    options: StreamPromptOptions = {},
    signal?: AbortSignal,
  ): EventStream<ModelStepEvent, StreamPromptResult> {
    const stream = new EventStream<ModelStepEvent, StreamPromptResult>();
    void (async () => {
      try {
        await this.refreshAuth();
        const inner = this.runController.streamPrompt(prompt, options, signal);
        void (async () => {
          for await (const ev of inner) {
            stream.push(ev);
          }
          stream.end(await inner.result());
        })();
      } catch (err) {
        stream.push({ type: "error", message: err instanceof Error ? err.message : String(err) });
        stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      }
    })();
    return stream;
  }

  streamPromptLifecycle(
    prompt: string,
    options: StreamPromptOptions = {},
    signal?: AbortSignal,
  ): EventStream<HostEvent, StreamPromptResult> {
    const stream = new EventStream<HostEvent, StreamPromptResult>();
    void (async () => {
      try {
        await this.refreshAuth();
        const inner = this.runController.streamPromptUnified(prompt, options, signal);
        void (async () => {
          for await (const ev of inner) {
            stream.push(ev);
          }
          stream.end(await inner.result());
        })();
      } catch (err) {
        stream.push({
          type: "turn_failed",
          session_id: this.sessionId,
          turn_id: "",
          error: err instanceof Error ? err.message : String(err),
          timestamp: Date.now(),
        });
        stream.end({ messages: [], appendedMessages: [], status: "error", sessionId: "" });
      }
    })();
    return stream;
  }
}
