// ---- Orchestrator facade — DI container + thin adapter around ActorSystem ----

import type {
  AgentSpec,
  AgentTask,
  AgentTaskId,
  ApprovalGateway,
  HostEventListener,
  OrchestratorRuntimeConfig,
  OrchModelConfig,
  OrchRunOptions,
  OrchRunResult,
  OrchState,
  ToolProvider,
  ToolSet,
} from "piko-orchestrator-protocol";
import type { OrchestratorEvent } from "../actors/state/index.js";
import { type EventStore, InMemoryEventStore } from "../actors/state/index.js";
import { ActorSystem } from "../kernel/actor-system.js";
import type { ModelStepExecutor } from "../model/index.js";
import { OrchToolProvider, ToolRegistryImpl } from "../tools/index.js";
import { registerAgent, unregisterAgent } from "./agent.js";
import type { OrchestratorContext, RunHandle } from "./context.js";
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

  eventStore: EventStore;

  // ---- DI container ----
  toolRegistry: ToolRegistryImpl;
  modelExecutor: ModelStepExecutor;
  latestModelConfig?: OrchModelConfig;
  defaultAgentId = "main";

  agentSpecs = new Map<string, AgentSpec>();
  runs = new Map<string, RunHandle>();
  allocatedTaskIds = new Set<string>();
  maxConcurrentAgents: number;

  constructor(
    modelExecutor?: ModelStepExecutor,
    config?: OrchModelConfig,
    runtimeConfig?: OrchestratorRuntimeConfig,
  ) {
    this.system = new ActorSystem();
    this.runId = `run_${Date.now()}`;
    this.modelExecutor = modelExecutor ?? ({} as ModelStepExecutor);
    this.latestModelConfig = config;
    const maxConcurrentAgents = runtimeConfig?.maxConcurrentAgents ?? Number.MAX_SAFE_INTEGER;
    if (maxConcurrentAgents <= 0 || !Number.isInteger(maxConcurrentAgents)) {
      throw new RangeError("maxConcurrentAgents must be a positive integer");
    }
    this.maxConcurrentAgents = maxConcurrentAgents;

    const store = new InMemoryEventStore(this.runId);
    this.eventStore = store;

    const emit = async (event: OrchestratorEvent) => {
      this.eventStore.append(event);
    };

    // ---- Init DI container ----
    this.toolRegistry = new ToolRegistryImpl(emit);

    // Auto-register built-in orch control tools (delegate, join, state, plan)
    this.toolRegistry.registerProvider(new OrchToolProvider(this));
  }

  createAgentDeps(): import("../actors/agent/index.js").AgentActorDeps {
    return {
      modelExecutor: this.modelExecutor,
      emit: async (event) => {
        this.eventStore.append(event);
      },
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
    this.eventStore.append(event);
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
