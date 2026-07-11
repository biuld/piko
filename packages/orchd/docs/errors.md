# Errors

> Status: current  
> Audience: both

Public error types returned by the Agent Runtime API and session observation streams.

## AgentApiError

Returned by `create_task`, `submit_input`, `control_task`, and related command methods.

| Variant | When |
|---|---|
| `TaskNotFound` | `task_id` not in supervisor registry |
| `SessionMismatch` | Request `session_id` does not match the task's session |
| `TaskClosed` | Input or control rejected because task is closed |
| `TaskTerminated` | Operation on a permanently terminated task |
| `InvalidState` | Control or input incompatible with current task/work state |
| `DuplicateRequest` | Same `request_id` retry — caller should use original receipt |
| `IdempotencyConflict` | Same `request_id` with a different payload |
| `InputRejected` | Validation failed (empty content, bad identity, etc.) |
| `PersistenceUnavailable` | PersistSink not configured or unreachable |
| `PersistenceFailed(String)` | Durable commit failed — no transcript append, no LLM step |
| `RuntimeUnavailable` | Supervisor or task handle not ready |
| `SnapshotRequired` | Caller must refresh snapshot before continuing (command path) |
| `Cancelled` | Operation aborted (e.g. shutdown, caller cancellation) |

### Persistence errors

`PersistenceFailed` and `PersistenceUnavailable` are distinct:

- **Unavailable** — infrastructure not wired (bootstrap misconfiguration, sink missing).
- **Failed** — commit was attempted and rejected or errored (storage I/O, validation, seq conflict).

On `PersistenceFailed`, the caller may retry with the **same** `request_id` and `message_id`. orchd and hostd deduplicate safely.

### Idempotency errors

- **DuplicateRequest** — success path for retries; inspect returned receipt rather than treating as failure.
- **IdempotencyConflict** — programmer error or client bug; do not retry blindly.

## SessionStreamError

Returned when `subscribe_session` stream ends with an error or when polling yields `Err`.

| Variant | When |
|---|---|
| `SnapshotRequired { reason }` | Client must fetch snapshot and resubscribe |
| `SessionClosed` | Session hub torn down (shutdown) |
| `RuntimeUnavailable` | Runtime not ready for streaming |
| `Internal { message }` | Unexpected hub or channel failure |

### SnapshotRequiredReason

| Reason | Client action |
|---|---|
| `EpochChanged` | Hub reset or session reattached — resubscribe from fresh snapshot cursor |
| `CursorExpired` | Retention window exceeded — fetch snapshot, discard stale cursor |
| `CursorUnknown` | Invalid or never-seen cursor — fetch snapshot |

Recommended recovery:

```text
session_snapshot(session_id)
  → subscribe_session(after = snapshot.cursor)
  → replay reliable events from snapshot + stream
```

Do not infer transcript state from deltas alone after `SnapshotRequired`.

## Mapping to hostd / TUI

hostd should:

- Surface `PersistenceFailed` to the user as a retryable turn failure.
- Map `TaskClosed` / `TaskTerminated` to appropriate session UI state.
- On subscription `SnapshotRequired`, re-fetch snapshot and update TUI cursor — without cancelling in-flight tasks.

Internal runtime errors (LLM provider failures, tool errors) become committed assistant/tool messages or work failure — not `AgentApiError`, unless the API call itself fails.

## Related reading

- [public-api.md](public-api.md) — command return types
- [persistence.md](persistence.md) — when `PersistenceFailed` occurs
- [events-and-observation.md](events-and-observation.md) — `SessionStreamError` and cursors
- [invariants.md](invariants.md) — fail-closed persistence rules
