# StateActor

`orchestrator:state` owns event ingestion, event log, reducer projection,
subscriptions, snapshots, and graph projection.

Messages:

```ts
type StateMsg =
  | { type: "ingest_event"; event: OrchestratorEvent }
  | { type: "snapshot" }
  | { type: "dump_events" }
  | { type: "render_graph" }
  | { type: "subscribe"; listener: OrchestratorEventListener }
  | { type: "unsubscribe"; subscriptionId: string };
```

Consistency rule:

```text
await emit(event) returns after StateActor has appended and reduced the event
await snapshot() observes every event whose emit() has already resolved
```

Reducer responsibilities:

- deterministic and side-effect free
- no `await`
- no actor messaging
- no scheduling decisions

The actor owns event ingestion. The pure reducer only folds one event envelope
into one state value.

