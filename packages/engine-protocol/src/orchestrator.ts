// ---- Orchestrator barrel ----
//
// Re-exports from the split sub-modules:
//   orchestrator-events.ts   — event types, meta, envelope, scheduler decision, listener
//   orchestrator-state.ts    — state types, approval state, AgentOrchestrator interface
//   orchestrator-graph.ts    — graph projection types
//   orchestrator-reducer.ts  — deterministic state reducer

export type {
  OrchestratorEvent,
  OrchestratorEventEnvelope,
  OrchestratorEventListener,
  OrchestratorEventMeta,
  SchedulerDecision,
} from "./orchestrator-events.js";
export type {
  OrchestratorGraph,
  OrchestratorGraphEdge,
  OrchestratorGraphNode,
} from "./orchestrator-graph.js";
export { reduceOrchestratorEvent } from "./orchestrator-reducer.js";
export type {
  AgentOrchestrator,
  ApprovalRuntimeState,
  OrchestratorState,
} from "./orchestrator-state.js";
