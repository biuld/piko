// ---- lifecycle subsystem ----

export type OrchLifecycleEvent =
  | { subsystem: "lifecycle"; type: "orchestrator_started"; runId: string }
  | { subsystem: "lifecycle"; type: "orchestrator_stopped"; runId: string; reason?: string };

import type { OrchestratorState } from "../state.js";

export function reduceLifecycle(
  state: OrchestratorState,
  event: OrchLifecycleEvent,
): OrchestratorState {
  switch (event.type) {
    case "orchestrator_started":
      return { ...state, runId: event.runId, status: "running" };
    case "orchestrator_stopped":
      return { ...state, status: "stopped" };
  }
}
