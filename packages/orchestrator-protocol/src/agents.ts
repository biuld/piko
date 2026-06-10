// ---- Agent type definitions ----

// ---- Agent status ----

export type AgentStatus = "idle" | "queued" | "running" | "blocked" | "waiting" | "failed";

/** How an agent runs concurrent tasks. */
export interface AgentConcurrencyPolicy {
  canRunInParallel?: boolean;
  requiresWriteLock?: boolean;
  maxConcurrentTasks?: number;
}

/** Static agent definition. */
export interface AgentSpec {
  id: string;
  name: string;
  role: string;
  description?: string;
  systemPrompt: string;
  model?: string;
  toolSetIds: string[];
  maxSteps?: number;
  concurrency?: AgentConcurrencyPolicy;
}

// ---- Agent runtime state ----

export interface AgentRuntimeState {
  id: string;
  spec: AgentSpec;
  status: AgentStatus;
  inbox: string[]; // AgentTaskId[]
  activeTaskId?: string;
  transcript: import("piko-engine-protocol").Message[];
  engineState?: unknown;
  lastWakeReason?: WakeReason;
}

// ---- Tasks ----

export type AgentTaskId = string;

export type TaskSource =
  | { kind: "user" }
  | { kind: "watch"; watchId: string }
  | { kind: "timer"; watchId: string }
  | { kind: "agent"; agentId: string; taskId: string }
  | { kind: "approval"; approvalId: string };

export type AgentTaskStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "blocked"
  | "cancelled";

export interface AgentTask {
  id?: AgentTaskId;
  targetAgentId: string;
  prompt: string;
  source: TaskSource;
  priority?: number;
  parentTaskId?: string;
  correlationId?: string;
}

export interface AgentTaskState {
  id: AgentTaskId;
  targetAgentId: string;
  prompt: string;
  source: TaskSource;
  status: AgentTaskStatus;
  priority: number;
  parentTaskId?: string;
  result?: AgentTaskResult;
  error?: string;
}

// ---- Task results ----

export interface AgentArtifact {
  id: string;
  kind: string;
  data: unknown;
}

export interface AgentTaskResult {
  summary: string;
  artifacts?: AgentArtifact[];
}

// ---- Watches and wakeups ----

export type AgentWatch =
  | {
      kind: "interval";
      id?: string;
      agentId: string;
      everyMs: number;
      prompt: string;
    }
  | {
      kind: "file_change";
      id?: string;
      agentId: string;
      paths: string[];
      debounceMs: number;
      prompt: string;
    }
  | {
      kind: "queue";
      id?: string;
      agentId: string;
      queueName: string;
    }
  | {
      kind: "dependency";
      id?: string;
      agentId: string;
      afterTaskId: string;
      prompt: string;
    };

export type AgentWatchId = string;

export interface AgentWatchState {
  id: AgentWatchId;
  agentId: string;
  kind: AgentWatch["kind"];
  active: boolean;
  lastFiredAt?: number;
}

export type WakeReason =
  | { kind: "user_task"; taskId: string }
  | { kind: "timer"; watchId: string }
  | { kind: "file_change"; watchId: string; paths: string[] }
  | { kind: "subagent_result"; fromAgentId: string; taskId: string }
  | { kind: "approval_resolved"; approvalId: string };

// ---- Locks ----

export type LockMode = "read" | "write";

export interface LockState {
  id: string;
  resource: string;
  mode: LockMode;
  holderAgentId?: string;
  holderTaskId?: string;
  queue: Array<{ agentId: string; taskId: string; mode: LockMode }>;
}
