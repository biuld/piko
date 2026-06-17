import type { ActorHandler } from "../../kernel/actor-system.js";
import { eventToHostEvent } from "./host-events.js";
import { buildGraph, buildSnapshot } from "./projections.js";
import { reduceStateEvent } from "./reducer.js";
import type {
  OrchestratorEvent,
  OrchestratorEventEnvelope,
  StateActorState,
  StateMsg,
} from "./types.js";

export function ingestStateEvent(
  state: StateActorState,
  event: OrchestratorEvent,
): OrchestratorEventEnvelope {
  state.seq++;
  const envelope: OrchestratorEventEnvelope = {
    id: `evt_${state.seq}`,
    runId: state.runId,
    seq: state.seq,
    time: Date.now(),
    event,
  };
  state.eventLog.push(envelope);
  reduceStateEvent(state, envelope);

  const hostEvent = eventToHostEvent(event, envelope, state);
  for (const listener of state.listeners.values()) {
    try {
      if (hostEvent) listener(hostEvent);
    } catch {
      // Listener failures must not disrupt orchestrator state updates.
    }
  }

  return envelope;
}

export function stateActor(state: StateActorState): ActorHandler<StateMsg> {
  return async (msg, ctx, meta) => {
    switch (msg.type) {
      case "ingest_event": {
        const envelope = ingestStateEvent(state, msg.event);
        ctx.reply(meta, envelope);
        return;
      }

      case "snapshot": {
        ctx.reply(meta, structuredClone(buildSnapshot(state)));
        return;
      }

      case "dump_events": {
        ctx.reply(meta, structuredClone(state.eventLog));
        return;
      }

      case "render_graph": {
        ctx.reply(meta, buildGraph(state));
        return;
      }

      case "subscribe": {
        const id = `sub_${state.nextSubId++}`;
        state.listeners.set(id, msg.listener);
        ctx.reply(meta, { id, unsubscribe: () => state.listeners.delete(id) });
        return;
      }

      case "unsubscribe": {
        state.listeners.delete(msg.subscriptionId);
        ctx.reply(meta, undefined);
        return;
      }
    }
  };
}

export function createStateActor(state: StateActorState) {
  return {
    handler: stateActor(state),
    state,
  };
}
