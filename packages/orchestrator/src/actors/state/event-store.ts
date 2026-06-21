import type { HostEventListener, OrchState } from "piko-orchestrator-protocol";
import { eventToHostEvent } from "./host-events.js";
import { buildGraph, buildSnapshot } from "./projections.js";
import { createInitialState, reduceStateEvent } from "./reducer.js";
import type { OrchestratorEvent, OrchestratorEventEnvelope, StateActorState } from "./types.js";

export interface EventStore {
  append(event: OrchestratorEvent): OrchestratorEventEnvelope;
  subscribe(listener: HostEventListener): () => void;
  snapshot(): OrchState;
  graph(): {
    nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
    edges: Array<{ from: string; to: string; label?: string }>;
  };
  dumpEvents(): OrchestratorEventEnvelope[];
}

export class InMemoryEventStore implements EventStore {
  private seq = 0;
  private readonly runId: string;
  private readonly eventLog: OrchestratorEventEnvelope[] = [];
  private readonly state: StateActorState;
  private readonly listeners = new Map<string, HostEventListener>();
  private nextSubId = 1;

  constructor(runId: string) {
    this.runId = runId;
    this.state = createInitialState(runId);
  }

  append(event: OrchestratorEvent): OrchestratorEventEnvelope {
    this.seq++;
    const envelope: OrchestratorEventEnvelope = {
      id: `evt_${this.seq}`,
      runId: this.runId,
      seq: this.seq,
      time: Date.now(),
      event,
    };
    this.eventLog.push(envelope);
    this.state.seq = this.seq;
    this.state.eventLog.push(envelope);
    reduceStateEvent(this.state, envelope);

    const hostEvent = eventToHostEvent(event, envelope, this.state);
    if (hostEvent) {
      for (const listener of this.listeners.values()) {
        try {
          listener(hostEvent);
        } catch {
          // Listener failures must not disrupt orchestrator state updates.
        }
      }
    }

    return envelope;
  }

  subscribe(listener: HostEventListener): () => void {
    const id = `sub_${this.nextSubId++}`;
    this.listeners.set(id, listener);
    return () => {
      this.listeners.delete(id);
    };
  }

  snapshot(): OrchState {
    return structuredClone(buildSnapshot(this.state));
  }

  graph() {
    return buildGraph(this.state);
  }

  dumpEvents(): OrchestratorEventEnvelope[] {
    return structuredClone(this.eventLog);
  }
}
