# EventStore

## InMemoryEventStore

`InMemoryEventStore` owns event ingestion, event log, reducer projection,
subscriptions, snapshots, and graph projection. It is a plain synchronous class —
no mailbox, no async serialization, no actor messages.

```ts
export interface EventStore {
  append(event: OrchestratorEvent): OrchestratorEventEnvelope;
  subscribe(listener: HostEventListener): () => void;
  snapshot(): OrchState;
  graph(): { nodes: ...; edges: ... };
  dumpEvents(): OrchestratorEventEnvelope[];
}
```

## How emit() Works

`emit()` in `AgentActorDeps` is wired to `eventStore.append()`:

```ts
const emit = async (event: OrchestratorEvent) => {
  this.eventStore.append(event);
};
```

`append()` is synchronous and executes immediately:
1. Assigns a monotonically increasing `seq` number
2. Pushes the event envelope to `eventLog`
3. Calls `reduceStateEvent(state, envelope)` in place
4. Notifies all subscribers synchronously (errors in listeners are swallowed)
5. Returns the envelope

Since `emit` is `async` but `append` is sync, `await emit(event)` resolves
in the same microtask tick. Snapshot consistency is guaranteed: any `snapshot()`
call after `await emit(event)` will observe that event.

## Consistency Rule

```text
append(event) reduces state synchronously
snapshot() after append() always observes that event
subscribers are called synchronously within append()
```

## Reducer Responsibilities

- Deterministic and side-effect free
- No `await`
- No actor messaging
- No scheduling decisions

The pure reducer (`reduceStateEvent`) folds one event envelope into the shared
mutable `StateActorState`. The store owns mutation; the reducer is a pure function.

## subscribe() / unsubscribe()

`subscribe(listener)` returns an unsubscribe function (no subscription ID needed):

```ts
const unsub = orchestrator.subscribe((event) => { ... });
// later:
unsub();
```

Listeners receive `HostEvent` objects (the public projection of internal
`OrchestratorEvent`s), not raw envelopes.
