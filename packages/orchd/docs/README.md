# orchd documentation

Design and public API documentation for orchd. Describes architecture and contracts — not source directory layout.

The public contract types live in the sibling crate [`orchd-api`](../../orchd-api/README.md). These docs cover both the contract and the default `orchd` implementation.

## Documents

| Document | Audience | Content |
|---|---|---|
| [overview.md](overview.md) | both | Role, goals, layering, design decisions |
| [core-model.md](core-model.md) | both | Task / Work / Message model and ownership |
| [public-api.md](public-api.md) | both | Agent Runtime four planes and crate exports |
| [host-integration.md](host-integration.md) | hostd | Bootstrap, turn paths, API mapping |
| [task-runtime.md](task-runtime.md) | orchd | Mailbox, input commit, task/work state machines |
| [events-and-observation.md](events-and-observation.md) | both | SessionOutput, hub, reliable vs realtime lanes |
| [persistence.md](persistence.md) | both | PersistSink, barrier, idempotency, storage contract |
| [invariants.md](invariants.md) | orchd | Rules that must hold across the runtime |
| [errors.md](errors.md) | both | `AgentApiError`, `SessionStreamError` |
| [testing.md](testing.md) | orchd | Integration test layout and conventions |

## Reading paths

**hostd integration**

```text
orchd-api README → overview → public-api → host-integration → persistence → events-and-observation
```

**orchd development**

```text
overview → core-model → task-runtime → persistence → events-and-observation → invariants
```

**Debugging API or stream failures**

```text
errors → invariants → testing
```

**Cross-crate identity and multi-agent semantics**

- [`docs/agent-identity.md`](../../../docs/agent-identity.md)
- [`docs/multi-agent-mental-model.md`](../../../docs/multi-agent-mental-model.md)
