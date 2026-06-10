// ============================================================================
// OrchestratorEvent — async runtime events (primitive, no overlap)
// ============================================================================

import type { OrchLifecycleEvent } from "./lifecycle/index.js";
import type { OrchResourceEvent } from "./resource/index.js";
import type { OrchSchedulerEvent } from "./scheduler/index.js";
import type { OrchTaskEvent } from "./task/index.js";

export type OrchestratorEvent =
  | OrchLifecycleEvent
  | OrchSchedulerEvent
  | OrchTaskEvent
  | OrchResourceEvent;

export type OrchestratorSubsystem = OrchestratorEvent["subsystem"];

// Re-export subsystem types
export type { OrchLifecycleEvent } from "./lifecycle/index.js";
export type { OrchResourceEvent, ResourceItem, ResourceResult } from "./resource/index.js";
export type { OrchSchedulerDecision, OrchSchedulerEvent } from "./scheduler/index.js";
export type { OrchTaskEvent } from "./task/index.js";

// ---- Envelope ----

import type { OrchestratorState } from "./state.js";

export interface OrchEventEnvelope {
  meta: OrchEventMeta;
  event: OrchestratorEvent;
}

export interface OrchEventMeta {
  eventId: string;
  timestamp: number;
  orchestratorRunId: string;
  correlationId?: string;
  parentTaskId?: string;
}

// ---- Listener ----

export type OrchEventListener = (envelope: OrchEventEnvelope, state: OrchestratorState) => void;
