import type { EngineRunSettings, Message, Model, ToolSet } from "piko-protocol";

// ---- Agent types ----

export type AgentStatus = "idle" | "running" | "failed" | "stopped";

export interface AgentConcurrencyPolicy {
  /** @deprecated Locks are provider-internal, not an orchestration concern. */
  requiresWriteLock?: boolean;
  maxConcurrentTasks?: number;
}

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

export interface AgentRuntimeState {
  id: string;
  spec: AgentSpec;
  status: AgentStatus;
  activeTaskId?: string;
  transcript: Message[];
}

// ---- Task types ----

export type AgentTaskId = string;

export type TaskSource = { type: "user" } | { type: "agent"; agentId: string; taskId: string };

export type AgentTaskStatus = "queued" | "running" | "completed" | "failed" | "cancelled";

export interface AgentTask {
  id?: AgentTaskId;
  targetAgentId: string;
  prompt: string;
  source: TaskSource;
  priority?: number;
  parentTaskId?: string;
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

export interface AgentArtifact {
  id: string;
  type: string;
  data: unknown;
}

export interface AgentTaskResult {
  summary: string;
  artifacts?: AgentArtifact[];
}

// ---- Host-visible streaming events ----

export type HostEvent =
  | { type: "token"; agentId: string; taskId: string; text: string }
  | {
      type: "thinking";
      agentId: string;
      taskId: string;
      text: string;
    }
  | {
      type: "tool_start";
      agentId: string;
      taskId: string;
      id: string;
      name: string;
      args: Record<string, unknown>;
    }
  | {
      type: "tool_end";
      agentId: string;
      taskId: string;
      id: string;
      name: string;
      result: unknown;
      isError: boolean;
    }
  | {
      type: "approval_needed";
      approvalId: string;
      agentId: string;
      taskId: string;
      toolName: string;
      toolArgs: Record<string, unknown>;
    }
  | {
      type: "approval_resolved";
      approvalId: string;
      agentId: string;
      taskId: string;
      decision: "accept" | "decline";
    }
  | { type: "task_started"; taskId: string; agentId: string }
  | {
      type: "task_completed";
      taskId: string;
      agentId: string;
      result: AgentTaskResult;
    }
  | {
      type: "task_failed";
      taskId: string;
      agentId: string;
      error: string;
    }
  | { type: "done"; status: string };

export type HostEventListener = (event: HostEvent) => void;

// ---- Runtime approval gateway ----

export interface ToolApprovalRequest {
  callId: string;
  agentId: string;
  taskId: string;
  toolName: string;
  toolArgs: Record<string, unknown>;
}

export type ToolApprovalDecision = "accept" | "decline";

export interface ApprovalGateway {
  requestToolApproval(request: ToolApprovalRequest): Promise<ToolApprovalDecision>;
}

// ---- Engine config passed by Host ----

export interface OrchEngineConfig {
  model: Model<string>;
  provider: import("piko-protocol").EngineProviderConfig;
  settings: EngineRunSettings;
  /** @deprecated Tools should be routed through ToolProviders, not an external handler. */
  externalToolHandler?: (name: string, args: Record<string, unknown>) => Promise<unknown>;
}

// ---- Run options / result ----

export interface OrchRunOptions {
  targetAgentId?: string;
  signal?: AbortSignal;
}

export interface OrchRunResult {
  messages: Message[];
  totalSteps: number;
  status: "completed" | "aborted" | "error" | "max_steps";
}

// ---- Snapshot state ----

export interface OrchState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  toolSets: Record<string, ToolSet>;
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
}

// ---- Orchestrator interface (facade) ----

export interface Orchestrator {
  registerAgent(spec: AgentSpec): void;
  unregisterAgent(agentId: string): void;
  registerToolSet(toolSet: ToolSet): void;
  unregisterToolSet(toolSetId: string): void;
  setEngineConfig(config: OrchEngineConfig): void;
  setApprovalGateway(gateway: ApprovalGateway | undefined): void;
  dispatch(task: AgentTask): Promise<AgentTaskId>;
  run(prompt: string, opts?: OrchRunOptions): Promise<OrchRunResult>;
  subscribe(listener: HostEventListener): () => void;
  snapshot(): OrchState;
}
