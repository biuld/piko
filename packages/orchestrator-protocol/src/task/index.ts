// ---- task subsystem ----

import type { AgentTaskResult, AgentTaskState } from "../agents.js";

export type OrchTaskEvent =
  | {
      subsystem: "task";
      type: "enqueued";
      task: AgentTaskState;
    }
  | {
      subsystem: "task";
      type: "started";
      taskId: string;
      agentId: string;
      stepIndex: number;
    }
  | {
      subsystem: "task";
      type: "completed";
      taskId: string;
      agentId: string;
      result: AgentTaskResult;
      totalSteps: number;
    }
  | {
      subsystem: "task";
      type: "failed";
      taskId: string;
      agentId: string;
      error: string;
    }
  | {
      subsystem: "task";
      type: "blocked";
      taskId: string;
      agentId: string;
      reason: "awaiting_resource" | "awaiting_lock" | "awaiting_approval" | "awaiting_subagent";
    };

import type { OrchestratorState } from "../state.js";

export function reduceTask(state: OrchestratorState, event: OrchTaskEvent): OrchestratorState {
  switch (event.type) {
    case "enqueued":
      return {
        ...state,
        tasks: { ...state.tasks, [event.task.id]: event.task },
      };

    case "started": {
      const task = state.tasks[event.taskId];
      if (!task) return state;
      return {
        ...state,
        tasks: { ...state.tasks, [event.taskId]: { ...task, status: "running" } },
      };
    }

    case "completed": {
      const task = state.tasks[event.taskId];
      if (!task) return state;
      return {
        ...state,
        tasks: {
          ...state.tasks,
          [event.taskId]: { ...task, status: "completed", result: event.result },
        },
      };
    }

    case "failed": {
      const task = state.tasks[event.taskId];
      if (!task) return state;
      return {
        ...state,
        tasks: {
          ...state.tasks,
          [event.taskId]: { ...task, status: "failed", error: event.error },
        },
      };
    }

    case "blocked": {
      const task = state.tasks[event.taskId];
      if (!task) return state;
      return {
        ...state,
        tasks: { ...state.tasks, [event.taskId]: { ...task, status: "blocked" } },
      };
    }
  }
}
