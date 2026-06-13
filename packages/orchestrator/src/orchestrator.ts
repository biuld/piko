// ---- Orchestrator facade — thin adapter around ActorSystem ----

import type { StatelessEngine, ToolProvider, ToolSet } from "piko-protocol";
import { createMainActor } from "./actors/main.js";
import type { OrchestratorEvent } from "./actors/state.js";
import { type OrchestratorEventEnvelope, stateActor } from "./actors/state.js";
import { createToolActor } from "./actors/tool.js";
import { type ActorHandler, ActorSystem } from "./kernel/actor-system.js";
import { OrchestratorToolProvider } from "./providers/orchestrator-provider.js";
import type {
  AgentSpec,
  AgentTask,
  AgentTaskId,
  AgentTaskState,
  ApprovalGateway,
  HostEventListener,
  OrchEngineConfig,
  OrchRunOptions,
  OrchRunResult,
  OrchState,
} from "./types.js";

export class Orchestrator {
  private system: ActorSystem;
  private runId: string;

  // Actor IDs
  private stateRef = "orchestrator:state";
  private mainRef = "orchestrator:main";
  private toolRef = "tool:registry";

  // Local cache for sync snapshot() — mirrors StateActor state, populated by events.
  private stateCache: {
    runId: string;
    status: "idle" | "running" | "stopping" | "stopped";
    agents: Record<string, import("./types.js").AgentRuntimeState>;
    tasks: Record<string, AgentTaskState>;
    toolSets: Record<string, ToolSet>;
  };

  constructor(engine?: StatelessEngine, config?: OrchEngineConfig) {
    this.system = new ActorSystem();
    this.runId = `run_${Date.now()}`;

    const emit = async (event: OrchestratorEvent) => {
      await this.system.ask(this.stateRef, {
        type: "ingest_event",
        event,
      });
    };

    // ---- Spawn StateActor ----
    const stateActorState = {
      runId: this.runId,
      status: "idle" as const,
      eventLog: [] as OrchestratorEventEnvelope[],
      seq: 0,
      agents: {} as Record<string, import("./types.js").AgentRuntimeState>,
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

    // ---- Spawn ToolActor ----
    const toolActor = createToolActor({ emit });
    this.system.spawn({
      id: this.toolRef,
      kind: "tool",
      handler: toolActor.handler as ActorHandler,
    });

    // ---- Register built-in providers ----
    this.system.send(this.toolRef, {
      type: "register_provider",
      provider: new OrchestratorToolProvider(this.system),
    });

    // ---- Spawn MainActor ----
    const mainActorState = createMainActor({
      actorSystem: this.system,
      stateActorId: this.stateRef,
      emit,
      createAgentDeps: () => ({
        engine: engine ?? ({} as StatelessEngine),
        emit,
        maxSteps: config?.settings?.maxSteps ?? 50,
        toolActorId: this.toolRef,
        actorSystem: this.system,
        engineConfig: config
          ? { model: config.model, provider: config.provider, settings: config.settings }
          : undefined,
      }),
    });

    this.system.spawn({
      id: this.mainRef,
      kind: "main",
      handler: mainActorState.handler as ActorHandler,
    });
  }

  // ---- Public API (all mutations go through actors; facade only caches) ----

  registerAgent(spec: AgentSpec): void {
    // Fire-and-forget to MainActor. Host must await run() before reading snapshot.
    this.system.send(this.mainRef, { type: "register_agent", spec });
  }

  unregisterAgent(agentId: string): void {
    this.system.send(this.mainRef, { type: "unregister_agent", agentId });
  }

  registerToolSet(toolSet: ToolSet): void {
    this.stateCache.toolSets[toolSet.id] = toolSet;
    this.system.send(this.toolRef, { type: "register_tool_set", toolSet });
  }

  unregisterToolSet(toolSetId: string): void {
    delete this.stateCache.toolSets[toolSetId];
    this.system.send(this.toolRef, { type: "unregister_tool_set", toolSetId });
  }

  setEngineConfig(config: OrchEngineConfig): void {
    this.system.send(this.mainRef, { type: "set_engine_config", config });
  }

  setApprovalGateway(gateway: ApprovalGateway | undefined): void {
    this.system.send(this.toolRef, { type: "set_approval_gateway", gateway });
  }

  registerProvider(provider: ToolProvider): void {
    this.system.send(this.toolRef, { type: "register_provider", provider });
  }

  async dispatch(task: AgentTask): Promise<AgentTaskId> {
    const taskId = task.id ?? `task_${Date.now()}_${Math.random().toString(36).slice(2)}`;
    task.id = taskId;
    await this.system.ask(this.mainRef, { type: "dispatch", task });
    return taskId;
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
}
