// ---- AgentOrchestrator — engine fully encapsulated, tick is the sole driver ----

import type {
  AgentOrchestrator as AgentOrchestratorInterface,
  AgentSpec,
  AgentTask,
  AgentTaskId,
  AgentTaskResult,
  AgentWatch,
  AgentWatchId,
  EngineApprovalResolution,
  EngineStepResult,
  EngineToolSet,
  LockMode,
  Message,
  OrchestratorEventEnvelope,
  OrchestratorEventListener,
  OrchestratorGraph,
  OrchestratorState,
  StatelessEngine,
  WakeReason,
} from "piko-engine-protocol";
import { renderGraph } from "../graph.js";
import type { OrchestratorCtx, OrchestratorEngineConfig } from "./context.js";
import { emitToCtx } from "./context.js";
import { executeAgentSteps } from "./engine-executor.js";
import { releaseLock, requestLock } from "./locks.js";
import { registerAgent, registerToolSet, unregisterAgent, unregisterToolSet } from "./registry.js";
import { schedule } from "./scheduler.js";
import { createOrchestratorState } from "./state-factory.js";
import { blockTask, completeTask, dispatch, failTask, wake } from "./tasks.js";
import { registerWatch, unregisterWatch } from "./watches.js";

// ---- Types ----

export interface OrchestratorRunResult {
  messages: Message[];
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps";
}

interface PendingResource {
  approvalId: string;
  taskId: string;
  details: unknown;
  engineState: unknown;
}

// ---- Orchestrator ----

export class AgentOrchestrator implements AgentOrchestratorInterface {
  private _ctx: OrchestratorCtx;
  private _pendingResources = new Map<string, PendingResource>();

  constructor(engine?: StatelessEngine, engineConfig?: OrchestratorEngineConfig, runId?: string) {
    this._ctx = {
      state: createOrchestratorState(runId),
      events: [],
      listeners: new Set(),
      engine,
      engineConfig,
    };
  }

  // ---- Lifecycle ----

  start(): void {
    if (this._ctx.state.status === "running") return;
    emitToCtx(this._ctx, { type: "orchestrator_started", runId: this._ctx.state.runId });
  }

  async stop(): Promise<void> {
    if (this._ctx.state.status !== "running") return;
    this._pendingResources.clear();
    emitToCtx(this._ctx, {
      type: "orchestrator_stopped",
      runId: this._ctx.state.runId,
      reason: "manual",
    });
  }

  // ---- Run (Host-facing convenience — drives tick loop) ----

  async run(
    prompt: string,
    options?: { targetAgentId?: string; signal?: AbortSignal },
  ): Promise<OrchestratorRunResult> {
    const target = options?.targetAgentId ?? "implementer";
    const signal = options?.signal;
    const maxSteps = this._ctx.engineConfig?.settings.maxSteps ?? 50;

    if (!this._ctx.state.agents[target]) {
      throw new Error(`Agent "${target}" not registered.`);
    }
    if (!this._ctx.engine) {
      throw new Error("No engine configured.");
    }

    this.start();
    this._pendingResources.clear();

    await dispatch(this._ctx, { targetAgentId: target, prompt, source: { kind: "user" } });

    let steps = 0;
    while (steps < maxSteps) {
      if (signal?.aborted) {
        this._cancelAll("Aborted");
        return this._collectResult("aborted");
      }
      if (this._allDone()) break;

      await this.tick(signal);
      steps++;
    }

    return this._collectResult(steps >= maxSteps ? "max_steps" : "completed");
  }

  /**
   * The core primitive: one scheduler + engine cycle.
   *
   * 1. schedule() — promotes idle agents with inbox tasks to running
   * 2. For each running agent (not blocked on approval), run one engine step
   *
   * Host calls this in a loop. Between ticks, Host processes events
   * (render, savePoint) and resolves approvals.
   */
  async tick(signal?: AbortSignal): Promise<void> {
    if (!this._ctx.engine) return;

    // Phase 1: promote idle agents
    schedule(this._ctx);

    // Phase 2: execute engine steps for all running agents
    await executeAgentSteps(this._ctx, signal, this._pendingResources);
  }

  // ---- Approval (called by Host between ticks) ----

  getPendingResources(): PendingResource[] {
    return [...this._pendingResources.values()];
  }

  async resolveResource(
    agentId: string,
    approvalId: string,
    decision: "accept" | "decline" | "acceptForSession",
    signal?: AbortSignal,
  ): Promise<void> {
    const pending = this._pendingResources.get(agentId);
    if (!pending || pending.approvalId !== approvalId) {
      throw new Error(`No matching approval for agent ${agentId}: ${approvalId}`);
    }

    const engine = this._ctx.engine;
    const agent = this._ctx.state.agents[agentId];

    if (engine?.resolveResource) {
      const stepId = `step-${pending.taskId}-resolve-${Date.now()}`;
      const resolution: EngineApprovalResolution = {
        runId: this._ctx.state.runId,
        stepId,
        approvalRequestId: approvalId,
        decision,
        transcript: agent?.transcript ?? [],
        engineState: pending.engineState,
      };

      this._pendingResources.delete(agentId);

      // resolveApproval emits its own engine_step_* events
      const stream = engine.resolveResource(resolution, signal);
      const result = await stream;
      this._appendTranscript(agentId, result);
    } else {
      this._pendingResources.delete(agentId);
    }

    emitToCtx(this._ctx, {
      type: "approval_resolved",
      agentId,
      taskId: pending.taskId,
      approvalId,
      decision,
    });

    if (decision === "decline") {
      failTask(this._ctx, pending.taskId, "User declined approval");
    }
  }

  // ---- Config ----

  setEngineConfig(config: OrchestratorEngineConfig): void {
    this._ctx.engineConfig = config;
  }

  reRegisterAgent(spec: AgentSpec): void {
    const existing = this._ctx.state.agents[spec.id];
    const prev = existing ?? {
      status: "idle" as const,
      inbox: [] as string[],
      activeTaskId: undefined as string | undefined,
      transcript: [] as Message[],
      engineState: undefined as unknown,
      lastWakeReason: undefined as WakeReason | undefined,
    };
    this.unregisterAgent(spec.id);
    this.registerAgent(spec);
    this._ctx.state.agents[spec.id] = { ...this._ctx.state.agents[spec.id], ...prev };
  }

  // ---- State queries (Host uses between ticks) ----

  isDone(): boolean {
    return this._allDone();
  }

  private _appendTranscript(agentId: string, result: EngineStepResult): void {
    const agent = this._ctx.state.agents[agentId];
    if (agent && result.appendedMessages.length > 0) {
      this._ctx.state.agents = {
        ...this._ctx.state.agents,
        [agentId]: {
          ...agent,
          transcript: [...agent.transcript, ...result.appendedMessages],
          engineState: result.engineState,
        },
      };
    }
  }

  private _allDone(): boolean {
    const tasks = Object.values(this._ctx.state.tasks);
    if (tasks.length === 0) return false;
    return tasks.every((t) => ["completed", "failed", "blocked", "cancelled"].includes(t.status));
  }

  private _cancelAll(reason: string): void {
    for (const task of Object.values(this._ctx.state.tasks)) {
      if (task.status === "queued" || task.status === "running") {
        failTask(this._ctx, task.id, reason);
      }
    }
  }

  private _collectResult(status: OrchestratorRunResult["status"]): OrchestratorRunResult {
    const allMessages: Message[] = [];
    for (const agent of Object.values(this._ctx.state.agents)) {
      allMessages.push(...agent.transcript);
    }
    return { messages: allMessages, totalSteps: Object.keys(this._ctx.state.tasks).length, status };
  }

  // ---- Delegates ----

  registerToolSet(toolSet: EngineToolSet): void {
    registerToolSet(this._ctx, toolSet);
  }
  unregisterToolSet(toolSetId: string): void {
    unregisterToolSet(this._ctx, toolSetId);
  }
  registerAgent(spec: AgentSpec): void {
    registerAgent(this._ctx, spec);
  }
  unregisterAgent(agentId: string): void {
    unregisterAgent(this._ctx, agentId);
  }

  async dispatch(task: AgentTask): Promise<AgentTaskId> {
    return dispatch(this._ctx, task);
  }
  async wake(agentId: string, reason: WakeReason): Promise<void> {
    return wake(this._ctx, agentId, reason);
  }

  completeTask(taskId: AgentTaskId, result: AgentTaskResult): void {
    completeTask(this._ctx, taskId, result);
  }
  failTask(taskId: AgentTaskId, error: string): void {
    failTask(this._ctx, taskId, error);
  }
  blockTask(taskId: AgentTaskId, reason: string): void {
    blockTask(this._ctx, taskId, reason);
  }

  requestLock(agentId: string, taskId: string, resource: string, mode: LockMode): boolean {
    return requestLock(this._ctx, agentId, taskId, resource, mode);
  }
  releaseLock(agentId: string, taskId: string, resource: string): void {
    releaseLock(this._ctx, agentId, taskId, resource);
  }

  registerWatch(watch: AgentWatch): AgentWatchId {
    return registerWatch(this._ctx, watch);
  }
  unregisterWatch(watchId: AgentWatchId): void {
    unregisterWatch(this._ctx, watchId);
  }

  subscribe(listener: OrchestratorEventListener): () => void {
    this._ctx.listeners.add(listener);
    return () => {
      this._ctx.listeners.delete(listener);
    };
  }

  snapshot(): OrchestratorState {
    return this._ctx.state;
  }
  dumpEvents(): OrchestratorEventEnvelope[] {
    return [...this._ctx.events];
  }
  renderGraph(): OrchestratorGraph {
    return renderGraph(this._ctx.state);
  }
}
