// ---- Orchestrator facade — DI container + thin adapter around ActorSystem ----

import type {
  AgentSpec,
  AgentTask,
  AgentTaskId,
  AgentTaskState,
  ApprovalGateway,
  HostEventListener,
  OrchModelConfig,
  OrchRunOptions,
  OrchRunResult,
  OrchState,
  ToolProvider,
  ToolSet,
} from "piko-orchestrator-protocol";
import { createMainActor } from "./actors/main.js";
import type { OrchestratorEvent } from "./actors/state.js";
import { type OrchestratorEventEnvelope, stateActor } from "./actors/state.js";
import { type ActorHandler, ActorSystem } from "./kernel/actor-system.js";
import type { ModelStepExecutor } from "./model/index.js";
import { OrchToolProvider, ToolRegistryImpl } from "./tools/index.js";

export class Orchestrator {
  private system: ActorSystem;
  private runId: string;

  // Actor IDs
  private stateRef = "orchestrator:state";
  private mainRef = "orchestrator:main";

  // ---- DI container ----
  private toolRegistry: ToolRegistryImpl;

  // Local cache for sync snapshot()
  private stateCache: {
    runId: string;
    status: "idle" | "running" | "stopping" | "stopped";
    agents: Record<string, import("piko-orchestrator-protocol").AgentRuntimeState>;
    tasks: Record<string, AgentTaskState>;
    toolSets: Record<string, ToolSet>;
  };

  constructor(modelExecutor?: ModelStepExecutor, config?: OrchModelConfig) {
    this.system = new ActorSystem();
    this.runId = `run_${Date.now()}`;

    const emit = async (event: OrchestratorEvent) => {
      await this.system.ask(this.stateRef, {
        type: "ingest_event",
        event,
      });
    };

    // ---- Init DI container ----
    this.toolRegistry = new ToolRegistryImpl(this.system, emit);

    // Auto-register built-in orch control tools (delegate, join, state, plan)
    this.toolRegistry.registerProvider(new OrchToolProvider(this));

    // ---- Spawn StateActor ----
    const stateActorState = {
      runId: this.runId,
      status: "idle" as const,
      eventLog: [] as OrchestratorEventEnvelope[],
      seq: 0,
      agents: {} as Record<string, import("piko-orchestrator-protocol").AgentRuntimeState>,
      tasks: {} as Record<string, AgentTaskState>,
      toolSets: {} as Record<string, ToolSet>,
      locks: {} as Record<string, unknown>,
      listeners: new Map<string, HostEventListener>(),
      nextSubId: 1,
      callMetas: new Map(),
    };
    this.stateCache = stateActorState;

    this.system.spawn({
      id: this.stateRef,
      kind: "state",
      handler: stateActor(stateActorState) as ActorHandler,
    });

    // ---- Spawn MainActor ----
    const mainActorState = createMainActor({
      actorSystem: this.system,
      stateActorId: this.stateRef,
      emit,
      createAgentDeps: () => ({
        modelExecutor: modelExecutor ?? ({} as ModelStepExecutor),
        emit,
        maxSteps: config?.settings?.maxSteps ?? 50,
        actorSystem: this.system,
        modelConfig: config
          ? { model: config.model, provider: config.provider, settings: config.settings }
          : undefined,
        toolRegistry: this.toolRegistry,
      }),
    });

    this.system.spawn({
      id: this.mainRef,
      kind: "main",
      handler: mainActorState.handler as ActorHandler,
    });
  }

  // ---- Public API ----

  registerAgent(spec: AgentSpec): void {
    this.system.send(this.mainRef, { type: "register_agent", spec });
  }

  unregisterAgent(agentId: string): void {
    this.system.send(this.mainRef, { type: "unregister_agent", agentId });
  }

  registerToolSet(toolSet: ToolSet): void {
    this.toolRegistry.registerToolSet(toolSet);
    this.stateCache.toolSets[toolSet.id] = toolSet;
  }

  unregisterToolSet(toolSetId: string): void {
    this.toolRegistry.unregisterToolSet(toolSetId);
    delete this.stateCache.toolSets[toolSetId];
  }

  setModelConfig(config: OrchModelConfig): void {
    this.system.send(this.mainRef, { type: "set_model_config", config });
  }

  setApprovalGateway(gateway: ApprovalGateway | undefined): void {
    this.toolRegistry.setApprovalGateway(gateway);
  }

  registerProvider(provider: ToolProvider): void {
    this.toolRegistry.registerProvider(provider);
  }

  // ---- Detached task tracking (for delegate_to_agent detach mode) ----
  private detachedTasks = new Map<
    string,
    { promise: Promise<unknown>; resolved: boolean; result?: unknown }
  >();

  async dispatch(task: AgentTask): Promise<AgentTaskId> {
    const taskId = task.id ?? `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
    task.id = taskId;
    await this.system.ask(this.mainRef, { type: "dispatch", task });
    return taskId;
  }

  /** Non-blocking dispatch: returns taskId immediately, result retrievable via joinTask. */
  async dispatchDetached(task: AgentTask): Promise<AgentTaskId> {
    const taskId = task.id ?? `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
    task.id = taskId;

    const resultPromise = this.system.ask(this.mainRef, { type: "dispatch", task });
    const handle = { promise: resultPromise, resolved: false, result: undefined as unknown };
    resultPromise
      .then((r) => {
        handle.result = r;
        handle.resolved = true;
      })
      .catch((err) => {
        handle.result = { error: String(err) };
        handle.resolved = true;
      });
    this.detachedTasks.set(taskId, handle);

    return taskId;
  }

  /**
   * Direct agent dispatch — bypasses MainActor to avoid deadlock when called
   * from within a tool execution (e.g. OrchToolProvider delegate_to_agent).
   *
   * Unlike dispatch(), this directly asks the target AgentActor without routing
   * through MainActor's mailbox. The caller is responsible for emitting
   * task_created and any other lifecycle events.
   */
  async delegateToAgent(task: AgentTask): Promise<{ taskId: string; result: unknown }> {
    const taskId = task.id ?? `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
    task.id = taskId;

    // Emit task_created directly (bypass MainActor)
    await this.system.ask(this.stateRef, {
      type: "ingest_event",
      event: { type: "task_created" as const, task },
    });

    // Ask the target AgentActor directly — no actor mailbox serialization
    const result = await this.system.ask<unknown>(`agent:${task.targetAgentId}`, {
      type: "dispatch",
      task,
    });

    return { taskId, result };
  }

  /**
   * Direct agent dispatch in detach mode — bypasses MainActor.
   * Returns taskId immediately; result retrievable via joinTask().
   */
  async delegateDetached(task: AgentTask): Promise<string> {
    const taskId = task.id ?? `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
    task.id = taskId;

    // Emit task_created directly
    await this.system.ask(this.stateRef, {
      type: "ingest_event",
      event: { type: "task_created" as const, task },
    });

    // Ask target AgentActor directly (fire-and-track)
    const resultPromise = this.system.ask<unknown>(`agent:${task.targetAgentId}`, {
      type: "dispatch",
      task,
    });

    const handle = { promise: resultPromise, resolved: false, result: undefined as unknown };
    resultPromise
      .then((r) => {
        handle.result = r;
        handle.resolved = true;
      })
      .catch((err) => {
        handle.result = { error: String(err) };
        handle.resolved = true;
      });
    this.detachedTasks.set(taskId, handle);

    return taskId;
  }

  /** Await the result of a previously detached task. */
  async joinTask(taskId: string): Promise<unknown> {
    const handle = this.detachedTasks.get(taskId);
    if (!handle) {
      throw new Error(`Detached task not found: ${taskId}`);
    }
    if (handle.resolved) {
      this.detachedTasks.delete(taskId);
      return handle.result;
    }
    const result = await handle.promise;
    this.detachedTasks.delete(taskId);
    return result;
  }

  /** Update the plan for an agent task (best-effort). */
  updatePlan(agentId: string, taskId: string, plan: unknown[]): void {
    this.system.send(this.stateRef, {
      type: "ingest_event",
      event: {
        type: "plan_updated",
        agentId,
        taskId,
        plan,
      },
    });
  }

  async run(prompt: string, opts?: OrchRunOptions): Promise<OrchRunResult> {
    return this.system.ask<OrchRunResult>(this.mainRef, {
      type: "run",
      prompt,
      options: opts,
    });
  }

  subscribe(listener: HostEventListener): () => void {
    this.system.send(this.stateRef, { type: "subscribe", listener });
    return () => {};
  }

  snapshot(): OrchState {
    return structuredClone({
      runId: this.runId,
      status: this.stateCache.status,
      toolSets: this.stateCache.toolSets,
      agents: this.stateCache.agents,
      tasks: this.stateCache.tasks,
    });
  }

  /** Get a graph representation of the orchestrator state (via StateActor). */
  async getGraph(): Promise<{
    nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
    edges: Array<{ from: string; to: string; label?: string }>;
  }> {
    return this.system.ask<{
      nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
      edges: Array<{ from: string; to: string; label?: string }>;
    }>(this.stateRef, { type: "render_graph" });
  }
}
