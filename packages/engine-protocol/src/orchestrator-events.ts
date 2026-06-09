// ---- Orchestrator event definitions ----

import type {
  AgentArtifact,
  AgentStatus,
  AgentTaskResult,
  AgentTaskState,
  AgentWatch,
  LockMode,
  WakeReason,
} from "./agents.js";
import type { EngineEvent, EngineStepStatus } from "./engine.js";

// ---- Event meta ----

export interface OrchestratorEventMeta {
  eventId: string;
  timestamp: number;
  orchestratorRunId: string;
  correlationId?: string;
  parentTaskId?: string;
}

// ---- Scheduler decision ----

export type SchedulerDecision =
  | { kind: "started"; agentId: string; taskId: string }
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

// ---- Event union ----

export type OrchestratorEvent =
  // Lifecycle
  | { type: "orchestrator_started"; runId: string }
  | { type: "orchestrator_stopped"; runId: string; reason?: string }
  // ToolSets
  | { type: "toolset_registered"; toolSetId: string; name: string }
  // Agents
  | { type: "agent_registered"; agentId: string; name: string; role: string; toolSetIds: string[] }
  | { type: "agent_unregistered"; agentId: string }
  | {
      type: "agent_status_changed";
      agentId: string;
      from: AgentStatus;
      to: AgentStatus;
      reason?: string;
    }
  // Watches
  | { type: "watch_registered"; watchId: string; agentId: string; kind: AgentWatch["kind"] }
  | { type: "watch_unregistered"; watchId: string }
  | { type: "watch_triggered"; watchId: string; agentId: string; reason: WakeReason }
  // Tasks
  | { type: "task_enqueued"; task: AgentTaskState }
  | { type: "task_started"; taskId: string; agentId: string }
  | { type: "task_completed"; taskId: string; agentId: string; result: AgentTaskResult }
  | { type: "task_failed"; taskId: string; agentId: string; error: string }
  | { type: "task_blocked"; taskId: string; agentId: string; reason: string }
  // Scheduler
  | { type: "scheduler_decision"; decision: SchedulerDecision }
  // Locks
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
  | { type: "lock_released"; lockId: string; agentId: string; taskId: string; resource: string }
  // Engine
  | { type: "engine_step_started"; agentId: string; taskId: string; stepId: string }
  | { type: "engine_event"; agentId: string; taskId: string; stepId: string; event: EngineEvent }
  | {
      type: "engine_step_completed";
      agentId: string;
      taskId: string;
      stepId: string;
      status: EngineStepStatus;
    }
  // Approval
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
  // Artifacts
  | { type: "artifact_produced"; agentId: string; taskId: string; artifact: AgentArtifact };

// ---- Event envelope ----

export interface OrchestratorEventEnvelope {
  meta: OrchestratorEventMeta;
  event: OrchestratorEvent;
}

// ---- Listener ----

import type { OrchestratorState } from "./orchestrator-state.js";

export type OrchestratorEventListener = (
  envelope: OrchestratorEventEnvelope,
  state: OrchestratorState,
) => void;
