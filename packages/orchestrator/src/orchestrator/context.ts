import type { OrchModelConfig } from "piko-orchestrator-protocol";
import type { AgentActorDeps } from "../actors/agent/types.js";
import type { OrchestratorEvent, StateActorState } from "../actors/state.js";
import type { ActorSystem } from "../kernel/actor-system.js";
import type { ToolRegistryImpl } from "../tools/tool-registry.js";

export interface OrchestratorContext {
  system: ActorSystem;
  runId: string;
  stateRef: string;
  toolRegistry: ToolRegistryImpl;
  modelExecutor: any;
  latestModelConfig?: OrchModelConfig;
  defaultAgentId: string;
  stateCache: StateActorState;
  detachedTasks: Map<string, { promise: Promise<unknown>; resolved: boolean; result?: unknown }>;
  createAgentDeps(): AgentActorDeps;
  emit(event: OrchestratorEvent): Promise<void>;
}
