# Core Model

> Status: current  
> Audience: both

## Objects

```text
AgentSpec     static capability template, keyed by agent_id
AgentTask     long-lived runtime instance, keyed by task_id
Turn          session-level user interaction, keyed by turn_id  (hostd)
Work          task-level input-driven execution cycle, keyed by work_id  (orchd)
Message       durable transcript fact, keyed by message_id
```

Identity details: [`docs/agent-identity.md`](../../../docs/agent-identity.md).

## Identities

| Identity | Meaning | Lifetime | Owner |
|---|---|---|---|
| `session_id` | hostd session | entire session | hostd |
| `agent_id` | AgentSpec template | static config | hostd config; orchd runtime copy |
| `task_id` | runtime instance | spans multiple works | orchd |
| `turn_id` | user/session interaction round | one TurnSubmit cycle | hostd |
| `work_id` | one input-driven execution cycle | accepted → terminal | orchd |
| `message_id` | transcript message | permanently stable | globally unique |
| `request_id` | API idempotency key | retry window | caller |

**Rule:** All runtime operations (steer, cancel, resume, view routing) must use `task_id`. Never use `agent_id` to locate a runtime instance.

`turn_id` and `work_id` are not aliases:

- **Turn** — hostd/session user interaction model
- **Work** — orchd/task execution model

One Turn may trigger multiple Works. One Work references at most one `source_turn_id`, or `None` (detached continuation, queue steer, etc.).

## Turn and Work

**Turn** (hostd):

```text
user/session input → root task processing → user-visible turn completion
```

**Work** (orchd):

```text
accepted task input → one or more model/tool steps → succeeded / failed / cancelled
```

Cardinality:

```text
Session 1 ── N Turn
Session 1 ── N Task
Task    1 ── N Work
Turn    1 ── 1..N Work
Work    ── source_turn_id ──> 0..1 Turn
```

Typical multi-agent turn:

```text
turn_42
├─ task_main     / work_main_7      (source_turn_id = turn_42)
├─ task_coder_1  / work_coder_3     (source_turn_id = turn_42)
└─ task_reviewer / work_reviewer_2  (source_turn_id = turn_42)
```

Detached child after turn completes:

```text
turn_42 completes
task_coder_1 / work_coder_4   (source_turn_id = None)
```

Implications:

- Turn completion does **not** terminate detached tasks it triggered.
- Work failure does **not** permanently kill a Task; the Task can accept new input and start a new Work.
- `CancelWork` cancels one work on one task — not the whole Turn.
- Turn state belongs to hostd; Work state belongs to orchd.

## Ownership

| State | Owner |
|---|---|
| AgentSpec registry | hostd config authority; orchd holds runtime copy |
| Active task handles | orchd runtime registry |
| In-memory task transcript | task runtime |
| Durable transcript | hostd JSONL (`tasks/{task_id}.jsonl`) |
| Task DAG durable projection | hostd |
| Task DAG live registry | orchd runtime registry |
| Session tree selection | hostd |
| TUI timeline / view | hostd projection + TUI local state |

Key boundaries:

- **orchd** decides what enters the transcript.
- **hostd** decides whether a committed fact is durable.
- The runtime registry does **not** own the transcript.
- Lifecycle metadata cannot replace transcript messages.

## Task vs Work lifecycle (summary)

**Task** — long-lived runtime handle: `Created → Idle ↔ Running → Closed / Terminated / Failed`

**Work** — single input execution: `Accepted → Running → Succeeded / Failed / Cancelled`

Full state machines: [task-runtime.md](task-runtime.md).

## Related reading

- [public-api.md](public-api.md) — operating on Tasks and Works via the API
- [`docs/multi-agent-mental-model.md`](../../../docs/multi-agent-mental-model.md) — spawn / steer / poll
