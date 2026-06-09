// ---- Orchestrator type definitions ----

import type {
  AgentArtifact,
  AgentRuntimeState,
  AgentSpec,
  AgentStatus,
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
import type { EngineEvent, EngineStepStatus } from "./engine.js";
import type { EngineToolSet } from "./tools.js";

// ---- Orchestrator events ----

export interface OrchestratorEventMeta {
  eventId: string;
  timestamp: number;
  orchestratorRunId: string;
  correlationId?: string;
  parentTaskId?: string;
}

export type SchedulerDecision =
  | {
      kind: "started";
      agentId: string;
      taskId: string;
    }
  | {
      kind: "skipped" | "deferred";
      agentId?: string;
      taskId?: string;
      reason:
        | "agent_busy"
        | "lock_unavailable"
        | "priority_lower_than_running"
        | "no_tasks"
        | "rate_limited"
        | "awaiting_approval";
    };

export type OrchestratorEvent =
  | { type: "orchestrator_started"; runId: string }
  | { type: "orchestrator_stopped"; runId: string; reason?: string }
  | { type: "toolset_registered"; toolSetId: string; name: string }
  | { type: "agent_registered"; agentId: string; name: string; role: string; toolSetIds: string[] }
  | { type: "agent_unregistered"; agentId: string }
  | {
      type: "agent_status_changed";
      agentId: string;
      from: AgentStatus;
      to: AgentStatus;
      reason?: string;
    }
  | { type: "watch_registered"; watchId: string; agentId: string; kind: AgentWatch["kind"] }
  | { type: "watch_unregistered"; watchId: string }
  | { type: "watch_triggered"; watchId: string; agentId: string; reason: WakeReason }
  | { type: "task_enqueued"; task: AgentTaskState }
  | { type: "task_started"; taskId: string; agentId: string }
  | { type: "task_completed"; taskId: string; agentId: string; result: AgentTaskResult }
  | { type: "task_failed"; taskId: string; agentId: string; error: string }
  | { type: "task_blocked"; taskId: string; agentId: string; reason: string }
  | { type: "scheduler_decision"; decision: SchedulerDecision }
  | {
      type: "lock_requested";
      lockId: string;
      agentId: string;
      taskId: string;
      resource: string;
      mode: LockMode;
    }
  | {
      type: "lock_acquired";
      lockId: string;
      agentId: string;
      taskId: string;
      resource: string;
      mode: LockMode;
    }
  | {
      type: "lock_released";
      lockId: string;
      agentId: string;
      taskId: string;
      resource: string;
    }
  | { type: "engine_step_started"; agentId: string; taskId: string; stepId: string }
  | { type: "engine_event"; agentId: string; taskId: string; stepId: string; event: EngineEvent }
  | {
      type: "engine_step_completed";
      agentId: string;
      taskId: string;
      stepId: string;
      status: EngineStepStatus;
    }
  | {
      type: "approval_requested";
      agentId: string;
      taskId: string;
      approvalId: string;
      details: unknown;
    }
  | {
      type: "approval_resolved";
      agentId: string;
      taskId: string;
      approvalId: string;
      decision: string;
    }
  | { type: "artifact_produced"; agentId: string; taskId: string; artifact: AgentArtifact };

export interface OrchestratorEventEnvelope {
  meta: OrchestratorEventMeta;
  event: OrchestratorEvent;
}

// ---- Orchestrator state ----

export interface ApprovalRuntimeState {
  id: string;
  agentId: string;
  taskId: string;
  details: unknown;
  status: "pending" | "resolved";
}

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

// ---- Graph projection ----

export interface OrchestratorGraphNode {
  id: string;
  kind: "agent" | "task" | "watch" | "lock" | "approval" | "artifact";
  status: string;
  label: string;
  metadata?: Record<string, unknown>;
}

export interface OrchestratorGraphEdge {
  from: string;
  to: string;
  kind:
    | "assigned_to"
    | "triggered"
    | "waiting_for"
    | "blocked_by"
    | "spawned"
    | "produced"
    | "requires";
}

export interface OrchestratorGraph {
  nodes: OrchestratorGraphNode[];
  edges: OrchestratorGraphEdge[];
}

// ---- Orchestrator listener ----

export type OrchestratorEventListener = (
  envelope: OrchestratorEventEnvelope,
  state: OrchestratorState,
) => void;

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

// ---- Reducer ----

export function reduceOrchestratorEvent(
  state: OrchestratorState,
  envelope: OrchestratorEventEnvelope,
): OrchestratorState {
  const { event } = envelope;
  const next = { ...state };

  switch (event.type) {
    case "orchestrator_started":
      return {
        ...state,
        runId: event.runId,
        status: "running",
      };

    case "orchestrator_stopped":
      return {
        ...state,
        status: "stopped",
      };

    case "toolset_registered": {
      // ToolSet registration is handled by insert, not event
      return state;
    }

    case "agent_registered": {
      return state;
    }

    case "agent_unregistered": {
      return state;
    }

    case "agent_status_changed": {
      const agent = next.agents[event.agentId];
      if (agent) {
        next.agents = {
          ...next.agents,
          [event.agentId]: { ...agent, status: event.to },
        };
      }
      return next;
    }

    case "watch_registered": {
      return state;
    }

    case "watch_unregistered": {
      return state;
    }

    case "watch_triggered": {
      return state;
    }

    case "task_enqueued": {
      next.tasks = { ...next.tasks, [event.task.id]: event.task };
      return next;
    }

    case "task_started": {
      const task = next.tasks[event.taskId];
      if (task) {
        next.tasks = { ...next.tasks, [event.taskId]: { ...task, status: "running" } };
      }
      return next;
    }

    case "task_completed": {
      const task = next.tasks[event.taskId];
      if (task) {
        next.tasks = {
          ...next.tasks,
          [event.taskId]: { ...task, status: "completed", result: event.result },
        };
      }
      return next;
    }

    case "task_failed": {
      const task = next.tasks[event.taskId];
      if (task) {
        next.tasks = {
          ...next.tasks,
          [event.taskId]: { ...task, status: "failed", error: event.error },
        };
      }
      return next;
    }

    case "task_blocked": {
      const task = next.tasks[event.taskId];
      if (task) {
        next.tasks = { ...next.tasks, [event.taskId]: { ...task, status: "blocked" } };
      }
      return next;
    }

    case "scheduler_decision": {
      return state;
    }

    case "lock_requested": {
      return state;
    }

    case "lock_acquired": {
      const lock = next.locks[event.lockId];
      if (lock) {
        next.locks = {
          ...next.locks,
          [event.lockId]: {
            ...lock,
            holderAgentId: event.agentId,
            holderTaskId: event.taskId,
          },
        };
      }
      return next;
    }

    case "lock_released": {
      const lock = next.locks[event.lockId];
      if (lock) {
        next.locks = {
          ...next.locks,
          [event.lockId]: {
            ...lock,
            holderAgentId: undefined,
            holderTaskId: undefined,
          },
        };
      }
      return next;
    }

    case "engine_step_started": {
      return state;
    }

    case "engine_event": {
      return state;
    }

    case "engine_step_completed": {
      return state;
    }

    case "approval_requested": {
      next.approvals = {
        ...next.approvals,
        [event.approvalId]: {
          id: event.approvalId,
          agentId: event.agentId,
          taskId: event.taskId,
          details: event.details,
          status: "pending",
        },
      };
      return next;
    }

    case "approval_resolved": {
      const approval = next.approvals[event.approvalId];
      if (approval) {
        next.approvals = {
          ...next.approvals,
          [event.approvalId]: { ...approval, status: "resolved" },
        };
      }
      return next;
    }

    case "artifact_produced": {
      next.artifacts = {
        ...next.artifacts,
        [event.artifact.id]: event.artifact,
      };
      return next;
    }

    default:
      return state;
  }
}
