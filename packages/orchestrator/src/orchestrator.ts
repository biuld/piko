// ---- AgentOrchestrator — engine fully encapsulated, tick is the sole driver ----

import type {
  EngineStepResult,
  EngineToolSet,
  Message,
  StatelessEngine,
} from "piko-engine-protocol";
import type {
  AgentOrchestrator as AgentOrchestratorInterface,
  AgentSpec,
  AgentTask,
  AgentTaskId,
  AgentTaskResult,
  AgentWatch,
  AgentWatchId,
  LockMode,
  OrchEventEnvelope,
  OrchEventListener,
  OrchestratorGraph,
  OrchestratorState,
  ResourceItem,
  ResourceResult,
  WakeReason,
} from "piko-orchestrator-protocol";
import type { OrchestratorCtx, OrchestratorEngineConfig } from "./context.js";
import { emitToCtx } from "./context.js";
import { renderGraph } from "./graph.js";
import { start as lifecycleStart, stop as lifecycleStop } from "./lifecycle/index.js";
import {
  registerAgent,
  registerToolSet,
  unregisterAgent,
  unregisterToolSet,
} from "./registry/index.js";
import { executeAgentSteps } from "./resource/engine-executor.js";
import { releaseLock, requestLock } from "./resource/locks.js";
import { schedule } from "./scheduler/index.js";
import { createOrchestratorState } from "./state-factory.js";
import { blockTask, completeTask, dispatch, failTask, wake } from "./task/index.js";
import { registerWatch, unregisterWatch } from "./watch/index.js";

// ---- Types ----

export interface OrchestratorRunResult {
  messages: Message[];
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps";
}

// ---- Orchestrator ----

export class AgentOrchestrator implements AgentOrchestratorInterface {
  private _ctx: OrchestratorCtx;

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
    lifecycleStart(this._ctx);
  }

  async stop(): Promise<void> {
    lifecycleStop(this._ctx);
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

  async tick(signal?: AbortSignal): Promise<void> {
    if (!this._ctx.engine) return;

    // Phase 1: promote idle agents
    schedule(this._ctx);

    // Phase 2: execute engine steps for all running agents
    await executeAgentSteps(this._ctx, signal);
  }

  // ---- Resource resolution (Host calls between ticks) ----

  /**
   * Resolve a resource request for tool/approval/lock/subagent.
   * Host invokes this after tool execution or user approval decision.
   */
  async resolveResource(
    agentId: string,
    taskId: string,
    item: ResourceItem,
    result: ResourceResult,
    signal?: AbortSignal,
  ): Promise<void> {
    const agent = this._ctx.state.agents[agentId];

    // Emit acquired
    emitToCtx(this._ctx, {
      subsystem: "resource",
      type: "acquired",
      agentId,
      taskId,
      item,
      result,
    });

    // Handle tool results: append to transcript
    if (item.kind === "tool" && result.kind === "tool") {
      const toolMsg: Message = {
        role: "toolResult",
        toolName: item.name,
        toolCallId: item.id,
        details: result.result,
        isError: result.isError,
      } as Message;

      if (agent) {
        this._ctx.state.agents = {
          ...this._ctx.state.agents,
          [agentId]: { ...agent, transcript: [...agent.transcript, toolMsg] },
        };
      }

      // Continue the step by feeding results back to the engine
      const engine = this._ctx.engine;
      if (engine?.resolveResource) {
        const stepResult = await engine.resolveResource(
          {
            runId: this._ctx.state.runId,
            stepId: `step-${taskId}-resume-${Date.now()}`,
            transcript: agent?.transcript ?? [],
            toolResults: [{ toolCallId: item.id, result: result.result, isError: result.isError }],
          },
          signal,
        );
        if (stepResult.appendedMessages.length > 0 && agent) {
          this._ctx.state.agents = {
            ...this._ctx.state.agents,
            [agentId]: {
              ...agent,
              transcript: [...agent.transcript, ...stepResult.appendedMessages],
              engineState: stepResult.engineState,
            },
          };
        }
      }
    }

    // Handle declined
    if (result.kind === "approval" && result.decision === "decline") {
      emitToCtx(this._ctx, {
        subsystem: "resource",
        type: "declined",
        agentId,
        taskId,
        item,
        reason: "User declined approval",
      });
      return;
    }

    // Check if all resources for this task are resolved
    emitToCtx(this._ctx, {
      subsystem: "resource",
      type: "resolved",
      agentId,
      taskId,
    });
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

  // ---- State queries ----

  isDone(): boolean {
    return this._allDone();
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

  subscribe(listener: OrchEventListener): () => void {
    this._ctx.listeners.add(listener);
    return () => {
      this._ctx.listeners.delete(listener);
    };
  }

  snapshot(): OrchestratorState {
    return this._ctx.state;
  }
  dumpEvents(): OrchEventEnvelope[] {
    return [...this._ctx.events];
  }
  renderGraph(): OrchestratorGraph {
    return renderGraph(this._ctx.state);
  }
}
