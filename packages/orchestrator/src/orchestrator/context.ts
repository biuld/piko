import type { AgentSpec, OrchModelConfig } from "piko-orchestrator-protocol";
import type { AgentActorDeps } from "../actors/agent/types.js";
import type { EventStore, OrchestratorEvent } from "../actors/state/index.js";
import type { ActorSystem } from "../kernel/actor-system.js";
import type { ToolRegistryImpl } from "../tools/tool-registry.js";

export interface RunHandle {
  taskId: string;
  agentId: string;
  actorId: string;
  status: "starting" | "running" | "cancelling" | "completed" | "failed" | "cancelled";
  /** Detached runs remain addressable through joinTask and must not be evicted. */
  retainForJoin: boolean;
  resultPromise: Promise<any>;
}

export interface OrchestratorContext {
  system: ActorSystem;
  runId: string;
  eventStore: EventStore;
  toolRegistry: ToolRegistryImpl;
  modelExecutor: any;
  latestModelConfig?: OrchModelConfig;
  defaultAgentId: string;
  agentSpecs: Map<string, AgentSpec>;
  runs: Map<string, RunHandle>;
  allocatedTaskIds: Set<string>;
  createAgentDeps(): AgentActorDeps;
  emit(event: OrchestratorEvent): Promise<void>;
}
