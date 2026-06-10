// ---- Orchestrator event reducer — delegates to subsystem reducers ----

import type { OrchEventEnvelope } from "./events.js";
import { reduceLifecycle } from "./lifecycle/index.js";
import { reduceResource } from "./resource/index.js";
import { reduceScheduler } from "./scheduler/index.js";
import type { OrchestratorState } from "./state.js";
import { reduceTask } from "./task/index.js";

export function reduceOrchestratorEvent(
  state: OrchestratorState,
  envelope: OrchEventEnvelope,
): OrchestratorState {
  const { event } = envelope;
  switch (event.subsystem) {
    case "lifecycle":
      return reduceLifecycle(state, event);
    case "scheduler":
      return reduceScheduler(state, event);
    case "task":
      return reduceTask(state, event);
    case "resource":
      return reduceResource(state, event);
  }
}
