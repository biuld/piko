// ---- scheduler subsystem ----

export type OrchSchedulerDecision =
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
        | "awaiting_approval"
        | "awaiting_resource";
    };

export type OrchSchedulerEvent = {
  subsystem: "scheduler";
  type: "decision";
  decision: OrchSchedulerDecision;
};

import type { OrchestratorState } from "../state.js";

export function reduceScheduler(
  state: OrchestratorState,
  _event: OrchSchedulerEvent,
): OrchestratorState {
  // Scheduler decisions are observability-only; they don't mutate state directly.
  return state;
}
