// ---- Orchestrator state & interface ----

import type {
  AgentArtifact,
  AgentRuntimeState,
  AgentSpec,
  AgentTask,
  AgentTaskId,
  AgentTaskState,
  AgentWatch,
  AgentWatchId,
  AgentWatchState,
  LockState,
  WakeReason,
} from "../agents.js";
import type { EngineToolSet } from "../tools.js";
import type { OrchestratorEventEnvelope, OrchestratorEventListener } from "./events.js";
import type { OrchestratorGraph } from "./graph.js";

// ---- Approval runtime state ----

export interface ApprovalRuntimeState {
  id: string;
  agentId: string;
  taskId: string;
  details: unknown;
  status: "pending" | "resolved";
}

// ---- Orchestrator state snapshot ----

export interface OrchestratorState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  toolSets: Record<string, EngineToolSet>;
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
  watches: Record<string, AgentWatchState>;
  locks: Record<string, LockState>;
  approvals: Record<string, ApprovalRuntimeState>;
  artifacts: Record<string, AgentArtifact>;
}

// ---- Orchestrator interface ----

export interface AgentOrchestrator {
  registerAgent(spec: AgentSpec): void;
  unregisterAgent(agentId: string): void;

  /** Register a ToolSet. Must happen before agents that reference it. */
  registerToolSet(toolSet: EngineToolSet): void;
  unregisterToolSet(toolSetId: string): void;

  dispatch(task: AgentTask): Promise<AgentTaskId>;
  wake(agentId: string, reason: WakeReason): Promise<void>;
  tick(now?: number): Promise<void>;

  registerWatch(watch: AgentWatch): AgentWatchId;
  unregisterWatch(watchId: AgentWatchId): void;

  subscribe(listener: OrchestratorEventListener): () => void;
  snapshot(): OrchestratorState;
  dumpEvents(): OrchestratorEventEnvelope[];
  renderGraph(): OrchestratorGraph;

  start(): void;
  stop(): Promise<void>;
}
