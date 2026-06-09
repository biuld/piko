import type {
  EngineProviderConfig,
  EngineRunSettings,
  OrchestratorEvent,
  OrchestratorEventEnvelope,
  OrchestratorEventListener,
  OrchestratorState,
  StatelessEngine,
} from "piko-engine-protocol";
import { reduceOrchestratorEvent } from "piko-engine-protocol";
import { v4Id } from "../id.js";

/**
 * Internal mutable context shared by all domain modules.
 */
export interface OrchestratorCtx {
  state: OrchestratorState;
  events: OrchestratorEventEnvelope[];
  listeners: Set<OrchestratorEventListener>;
  engine?: StatelessEngine;
  engineConfig?: OrchestratorEngineConfig;
}

export interface OrchestratorEngineConfig {
  model: import("piko-engine-protocol").Model<string>;
  provider: EngineProviderConfig;
  settings: EngineRunSettings;
  externalToolHandler?: (name: string, args: Record<string, unknown>) => Promise<unknown>;
  /** Max concurrent engine steps across all agents. Default: no limit. */
  maxConcurrentSteps?: number;
}

export function emitToCtx(ctx: OrchestratorCtx, event: OrchestratorEvent): void {
  const envelope: OrchestratorEventEnvelope = {
    meta: {
      eventId: v4Id("evt"),
      timestamp: Date.now(),
      orchestratorRunId: ctx.state.runId,
    },
    event,
  };
  ctx.events.push(envelope);
  ctx.state = reduceOrchestratorEvent(ctx.state, envelope);
  for (const listener of ctx.listeners) {
    try {
      listener(envelope, ctx.state);
    } catch {
      // Listener errors must not break the orchestrator
    }
  }
}
