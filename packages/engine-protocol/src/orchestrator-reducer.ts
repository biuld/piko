// ---- Orchestrator event reducer ----
//
// Deterministic reducer: (State, Event) → State.
// All state transitions must have a corresponding event.

import type { OrchestratorEventEnvelope } from "./orchestrator-events.js";
import type { OrchestratorState } from "./orchestrator-state.js";

export function reduceOrchestratorEvent(
  state: OrchestratorState,
  envelope: OrchestratorEventEnvelope,
): OrchestratorState {
  const { event } = envelope;
  const next = { ...state };

  switch (event.type) {
    case "orchestrator_started":
      return { ...state, runId: event.runId, status: "running" };

    case "orchestrator_stopped":
      return { ...state, status: "stopped" };

    case "toolset_registered":
      // ToolSet registration is handled by insert, not event
      return state;

    case "agent_registered":
    case "agent_unregistered":
      return state;

    case "agent_status_changed": {
      const agent = next.agents[event.agentId];
      if (agent) {
        next.agents = { ...next.agents, [event.agentId]: { ...agent, status: event.to } };
      }
      return next;
    }

    case "watch_registered":
    case "watch_unregistered":
    case "watch_triggered":
      return state;

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

    case "scheduler_decision":
    case "lock_requested":
      return state;

    case "lock_acquired": {
      const lock = next.locks[event.lockId];
      if (lock) {
        next.locks = {
          ...next.locks,
          [event.lockId]: { ...lock, holderAgentId: event.agentId, holderTaskId: event.taskId },
        };
      }
      return next;
    }

    case "lock_released": {
      const lock = next.locks[event.lockId];
      if (lock) {
        next.locks = {
          ...next.locks,
          [event.lockId]: { ...lock, holderAgentId: undefined, holderTaskId: undefined },
        };
      }
      return next;
    }

    case "engine_step_started":
    case "engine_event":
    case "engine_step_completed":
      return state;

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
      next.artifacts = { ...next.artifacts, [event.artifact.id]: event.artifact };
      return next;
    }

    default:
      return state;
  }
}
