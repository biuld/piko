// ---- Orchestrator state & interface ----

import type {
  AgentArtifact,
  AgentRuntimeState,
  AgentSpec,
  AgentTask,
  AgentTaskId,
  AgentTaskResult,
  AgentTaskState,
  AgentWatch,
  AgentWatchId,
  AgentWatchState,
  LockMode,
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
  /** Re-register an agent preserving runtime state. */
  reRegisterAgent(spec: AgentSpec): void;

  /** Register a ToolSet. Must happen before agents that reference it. */
  registerToolSet(toolSet: EngineToolSet): void;
  unregisterToolSet(toolSetId: string): void;

  dispatch(task: AgentTask): Promise<AgentTaskId>;
  wake(agentId: string, reason: WakeReason): Promise<void>;
  tick(signal?: AbortSignal): Promise<void>;
  /** Run a prompt through the full agent loop. */
  run(
    prompt: string,
    options?: { targetAgentId?: string; signal?: AbortSignal },
  ): Promise<{
    messages: import("../types.js").Message[];
    totalSteps: number;
    status: string;
    taskId?: string;
  }>;

  registerWatch(watch: AgentWatch): AgentWatchId;
  unregisterWatch(watchId: AgentWatchId): void;

  subscribe(listener: OrchestratorEventListener): () => void;
  snapshot(): OrchestratorState;
  dumpEvents(): OrchestratorEventEnvelope[];
  renderGraph(): OrchestratorGraph;
  isDone(): boolean;

  start(): void;
  stop(): Promise<void>;

  /** Update engine configuration before each run. */
  setEngineConfig(config: {
    model: import("../types.js").Model<string>;
    provider: import("../engine.js").EngineProviderConfig;
    settings: import("../engine.js").EngineRunSettings;
    externalToolHandler?: (name: string, args: Record<string, unknown>) => Promise<unknown>;
    maxConcurrentSteps?: number;
  }): void;

  requestLock(agentId: string, taskId: string, resource: string, mode: LockMode): boolean;
  releaseLock(agentId: string, taskId: string, resource: string): void;

  completeTask(taskId: AgentTaskId, result: AgentTaskResult): void;
  failTask(taskId: AgentTaskId, error: string): void;
  blockTask(taskId: AgentTaskId, reason: string): void;

  getPendingResources(): {
    approvalId: string;
    taskId: string;
    details: unknown;
    engineState: unknown;
  }[];
  resolveResource(
    agentId: string,
    approvalId: string,
    decision: "accept" | "decline" | "acceptForSession",
    signal?: AbortSignal,
  ): Promise<void>;
}
