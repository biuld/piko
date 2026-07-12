# Persistence

> Status: current  
> Audience: both

How orchd commits durable facts, enforces the write-before-LLM barrier, and what hostd must provide.

## PersistSink

orchd calls; hostd implements:

```rust
#[async_trait]
pub trait PersistSink: Send + Sync {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError>;
    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError>;
    async fn commit_work_event(&self, event: WorkEventCommit) -> Result<PersistAck, PersistError>;
}
```

```rust
pub struct PersistAck {
    pub session_id: String,
    pub task_id: String,
    pub message_id: Option<String>,
    pub task_seq: u64,
}
```

The trait lives in orchd (not `piko-protocol`) because protocol carries serializable DTOs only, not runtime side-effect ports.

Exposed to hostd as `orchd_api::PersistSink`.

## Persistence barrier

Enqueueing a persist event ≠ durable write. For user messages to be durable **before** an LLM step, orchd awaits `PersistAck`:

```text
orchd requests commit_message
  → hostd validates identity and task_seq order
  → hostd appends to task shard JSONL
  → hostd updates HostState / manifest (barrier projection)
  → hostd returns PersistAck
  → orchd appends in-memory transcript
  → MessageCommitted becomes observable
  → orchd starts LLM step
```

On failure: no transcript append, no LLM call, return `PersistenceFailed`.

Emitting an internal persist event without ack only guarantees **ordering**, not **durability before LLM**. Tests and docs must distinguish the two.

## Barrier vs observation (hostd)

Two phases must not be conflated:

| Phase | When | hostd responsibility |
|---|---|---|
| **Barrier** | Inside `PersistSink::commit_*`, before `PersistAck` | Write JSONL; update `HostState` and manifest so memory matches disk |
| **Observation** | On `SessionEvent::MessageCommitted` / `ToolCommitted` | Read JSONL via `TaskRepository`; emit `TranscriptCommitted` to TUI only |

Observation does **not** append to `HostState` again (invariant #18). The barrier already projected host-visible entries. Full design: [turn lifecycle and live projection](../../../docs/turn-lifecycle-and-live-projection.md).

## Session-scoped PersistSink

Production hostd binds **one** `Arc<dyn PersistSink>` per open session at `SessionCreate` / `SessionOpen`. Every turn calls `set_persist_sink` with the same `Arc`. orchd task runtimes hold a `SharedPersistSink` reference to the supervisor slot and resolve the current sink on each commit.

## Idempotency

| Key | Scope |
|---|---|
| `request_id` | API operation idempotency |
| `message_id` | Transcript message idempotency |

Rules:

- Same `request_id + task_id` retry → return original `InputReceipt` (`Duplicate`).
- Same `message_id` → do not append twice.
- Same `request_id` with different payload → `IdempotencyConflict`.

hostd `TaskRepository` and orchd command layer both enforce these checks.

## Per-task sequence

Every durable fact on a task carries monotonic `task_seq`:

```text
TaskCreated              seq 1
initial MessageCommitted seq 2
WorkStarted              seq 3
assistant committed      seq 4
tool call committed      seq 5
tool result committed    seq 6
WorkSucceeded / Idle     seq 7
next user message        seq 8
```

Used for: gap detection, per-task replay, idempotent commit, reliable event ordering.

hostd may also maintain a session-global view sequence; that is independent of `task_seq`.

## Storage contract (hostd)

orchd depends on hostd storage layout but does not implement it.

### Per-task shard

One task → one JSONL file. Routing uses **`task_id` only**, never `agent_id`.

```text
session/
├── session.json
└── tasks/
    ├── {task-id-1}.jsonl
    ├── {task-id-2}.jsonl
    └── …
```

Rules:

1. Root task uses `tasks/{root-task-id}.jsonl` — no special `main.jsonl` shard.
2. All message types (user, assistant, tool call, tool result) for a task live in one shard.
3. `parent_message_id` references only messages within the same task shard.
4. Multiple tasks with the same `agent_id` use separate files.

### Per-task head

Separate from session tree selection:

```text
session current_leaf_id     → user's selected node in session tree (hostd)
task_heads[task_id]         → last committed transcript message for that task
```

Each commit: `parent = task_heads[task_id]` → append → update head.

### Message entry fields

Every committed transcript entry must include:

```text
id, parent_id, task_id, agent_id, work_id, task_seq, timestamp, message
```

Entries missing `task_id` or `agent_id` are invalid (fail closed).

### Session manifest

`session.json` holds session metadata and a rebuildable task index — not transcript messages.

```text
session metadata     → session.json authoritative
task messages        → tasks/{task_id}.jsonl authoritative
session.json.tasks   → projection rebuildable from shards
```

### Recovery payload

hostd loads shards and passes resume state to orchd:

```rust
pub struct TaskResumeState {
    pub transcript: Vec<Message>,
    pub head_message_id: Option<MessageId>,
    pub last_task_seq: u64,
    pub committed_message_ids: Vec<MessageId>,
}
```

Used with `CreateTaskRequest.resume` to reattach a runtime without replaying historical input.

Recovery principles:

- Transcript recovery reads **MessageEntry** facts from task shards.
- UI replay is a projection of committed messages.
- Do **not** recover user content from `TaskEvent::Created.prompt`, display events, or manifest audit fields.

## Commit failure semantics

If durable append succeeds but manifest projection fails, the operation is not "undone." Retries must detect the existing fact, skip duplicate append, repair projection, and return the original ack.

## Related reading

- [task-runtime.md](task-runtime.md) — when commit runs in the task loop
- [host-integration.md](host-integration.md) — hostd `TaskRepository` wiring
- [invariants.md](invariants.md) — persistence invariants
