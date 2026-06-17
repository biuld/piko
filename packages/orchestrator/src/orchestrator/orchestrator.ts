// ---- Orchestrator facade — DI container + thin adapter around ActorSystem ----

import type {
  AgentSpec,
  AgentTask,
  AgentTaskId,
  ApprovalGateway,
  HostEventListener,
  OrchModelConfig,
  OrchRunOptions,
  OrchRunResult,
  OrchState,
  ToolProvider,
  ToolSet,
} from "piko-orchestrator-protocol";
import type { OrchestratorEvent } from "../actors/state.js";
import {
  createInitialState,
  ingestStateEvent,
  type StateActorState,
  stateActor,
} from "../actors/state.js";
import { type ActorHandler, ActorSystem } from "../kernel/actor-system.js";
import type { ModelStepExecutor } from "../model/index.js";
import { OrchToolProvider, ToolRegistryImpl } from "../tools/index.js";
import { registerAgent, unregisterAgent } from "./agent.js";
import type { OrchestratorContext } from "./context.js";
import { getGraph, snapshot, subscribe, updatePlan } from "./state.js";
import {
  cancelTask,
  delegateDetached,
  delegateToAgent,
  dispatch,
  dispatchDetached,
  joinTask,
  run,
} from "./task.js";
import {
  registerProvider,
  registerToolSet,
  setApprovalGateway,
  setModelConfig,
  unregisterToolSet,
} from "./tool.js";

export class Orchestrator implements OrchestratorContext {
  system: ActorSystem;
  runId: string;

  // Actor IDs
  stateRef = "orchestrator:state";

  // ---- DI container ----
  toolRegistry: ToolRegistryImpl;
  modelExecutor: ModelStepExecutor;
  latestModelConfig?: OrchModelConfig;
  defaultAgentId = "main";

  // Local cache for sync snapshot()
  stateCache: StateActorState;

  // Detached task tracking
  detachedTasks = new Map<
    string,
    { promise: Promise<unknown>; resolved: boolean; result?: unknown }
  >();

  constructor(modelExecutor?: ModelStepExecutor, config?: OrchModelConfig) {
    this.system = new ActorSystem();
    this.runId = `run_${Date.now()}`;
    this.modelExecutor = modelExecutor ?? ({} as ModelStepExecutor);
    this.latestModelConfig = config;

    const stateActorState = createInitialState(this.runId);
    this.stateCache = stateActorState;

    this.system.spawn({
      id: this.stateRef,
      kind: "state",
      handler: stateActor(stateActorState) as ActorHandler,
    });

    const emit = async (event: OrchestratorEvent) => {
      ingestStateEvent(this.stateCache, event);
    };

    // ---- Init DI container ----
    this.toolRegistry = new ToolRegistryImpl(this.system, emit);

    // Auto-register built-in orch control tools (delegate, join, state, plan)
    this.toolRegistry.registerProvider(new OrchToolProvider(this));
  }

  createAgentDeps(): import("../actors/agent/index.js").AgentActorDeps {
    return {
      modelExecutor: this.modelExecutor,
      emit: async (event) => {
        ingestStateEvent(this.stateCache, event);
      },
      maxSteps: this.latestModelConfig?.settings?.maxSteps ?? 50,
      actorSystem: this.system,
      modelConfig: this.latestModelConfig
        ? {
            model: this.latestModelConfig.model,
            provider: this.latestModelConfig.provider,
            settings: this.latestModelConfig.settings,
          }
        : undefined,
      toolRegistry: this.toolRegistry,
    };
  }

  async emit(event: OrchestratorEvent): Promise<void> {
    ingestStateEvent(this.stateCache, event);
  }

  // ---- Public API ----

  registerAgent(spec: AgentSpec): void {
    registerAgent(this, spec);
  }

  unregisterAgent(agentId: string): void {
    unregisterAgent(this, agentId);
  }

  registerToolSet(toolSet: ToolSet): void {
    registerToolSet(this, toolSet);
  }

  unregisterToolSet(toolSetId: string): void {
    unregisterToolSet(this, toolSetId);
  }

  setModelConfig(config: OrchModelConfig): void {
    this.latestModelConfig = config;
    setModelConfig(this, config);
  }

  setApprovalGateway(gateway: ApprovalGateway | undefined): void {
    setApprovalGateway(this, gateway);
  }

  registerProvider(provider: ToolProvider): void {
    registerProvider(this, provider);
  }

  async dispatch(task: AgentTask): Promise<AgentTaskId> {
    return dispatch(this, task);
  }

  async dispatchDetached(task: AgentTask): Promise<AgentTaskId> {
    return dispatchDetached(this, task);
  }

  async delegateToAgent(task: AgentTask): Promise<{ taskId: string; result: unknown }> {
    return delegateToAgent(this, task);
  }

  async delegateDetached(task: AgentTask): Promise<string> {
    return delegateDetached(this, task);
  }

  async joinTask(taskId: string): Promise<unknown> {
    return joinTask(this, taskId);
  }

  updatePlan(agentId: string, taskId: string, plan: unknown[]): void {
    updatePlan(this, agentId, taskId, plan);
  }

  async run(prompt: string, opts?: OrchRunOptions): Promise<OrchRunResult> {
    return run(this, prompt, opts);
  }

  async cancelTask(taskId: string, reason?: string): Promise<void> {
    return cancelTask(this, taskId, reason);
  }

  subscribe(listener: HostEventListener): () => void {
    return subscribe(this, listener);
  }

  snapshot(): OrchState {
    return snapshot(this);
  }

  async getGraph(): Promise<{
    nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
    edges: Array<{ from: string; to: string; label?: string }>;
  }> {
    return getGraph(this);
  }
}
