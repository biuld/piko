// ---- Orchestrator state & interface ----

import type { EngineToolSet } from "piko-engine-protocol";
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
} from "./agents.js";
import type { OrchEventEnvelope, OrchEventListener } from "./events.js";
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
    messages: import("piko-engine-protocol").Message[];
    totalSteps: number;
    status: string;
    taskId?: string;
  }>;

  registerWatch(watch: AgentWatch): AgentWatchId;
  unregisterWatch(watchId: AgentWatchId): void;

  subscribe(listener: OrchEventListener): () => void;
  snapshot(): OrchestratorState;
  dumpEvents(): OrchEventEnvelope[];
  renderGraph(): OrchestratorGraph;
  isDone(): boolean;

  start(): void;
  stop(): Promise<void>;

  /** Update engine configuration before each run. */
  setEngineConfig(config: {
    model: import("piko-engine-protocol").Model<string>;
    provider: import("piko-engine-protocol").EngineProviderConfig;
    settings: import("piko-engine-protocol").EngineRunSettings;
    externalToolHandler?: (name: string, args: Record<string, unknown>) => Promise<unknown>;
    maxConcurrentSteps?: number;
  }): void;

  requestLock(agentId: string, taskId: string, resource: string, mode: LockMode): boolean;
  releaseLock(agentId: string, taskId: string, resource: string): void;

  completeTask(taskId: AgentTaskId, result: AgentTaskResult): void;
  failTask(taskId: AgentTaskId, error: string): void;
  blockTask(taskId: AgentTaskId, reason: string): void;

  resolveResource(
    agentId: string,
    taskId: string,
    item: import("./resource/index.js").ResourceItem,
    result: import("./resource/index.js").ResourceResult,
    signal?: AbortSignal,
  ): Promise<void>;
}
