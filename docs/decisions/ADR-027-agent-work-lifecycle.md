# ADR-027: Separate Agent-work primitives from derived scopes

> Status: proposed
> Date: 2026-08-29
> Supersedes in part: [ADR-025](ADR-025-authoritative-agent-lifecycle.md) as a core-domain hierarchy

## Context

piko currently describes Agent work through AgentInstance, Turn, Run,
Execution, and ModelStep. All are useful, but they do not belong at the same
abstraction layer.

The old `Turn → Run → Execution → ModelStep` hierarchy treats product,
logical-work, and runtime-operation scopes as if each were an independently
authoritative entity. That creates overlapping lifecycle state:

- detached child and system work has no natural Turn;
- Run and Execution are normally one-to-one and repeat the same status;
- hostd, orchd, and clients each combine different subsets to decide whether
  an Agent is active;
- queues and accepted steers introduce more state outside the hierarchy.

A stable ID or a useful control target does not by itself make something a
primitive. Stable derived views can also be addressed. We need to distinguish
irreducible facts from scopes reconstructed from those facts.

## Decision

Use two explicit modeling layers.

### Layer 1: primitives and facts

```text
AgentInstance
├── AgentInput
└── ModelStep
    └── Message / Thought / ToolCall / ToolResult

Lifecycle and correlation facts connect these primitives:
input admitted / disposition changed
step committed
tool and pending-action facts
processing interrupted / terminal outcome
```

- **AgentInstance** is the durable identity and admission serialization
  boundary for one collaborator.
- **AgentInput** is an immutable, idempotent stimulus. It can remain pending,
  start work, steer a later ModelStep, or be cancelled before application.
- **ModelStep** is the atomic model request/response boundary. It relates
  assistant output and tool declarations and is independently required for
  recovery and reliable observation.
- Messages, tool activity, pending actions, and terminal outcome records are
  facts attached by Agent, input, step, and causal identities. They are not
  forced through another mutable lifecycle aggregate.

Primitive storage records what happened. It does not persist every useful
interpretation of those facts as another state machine.

### Layer 2: derived scopes and views

```text
Turn       = user-origin input(s) + correlated conversation/work facts
Run        = root input + causally connected inputs/steps/actions + outcome
Execution  = one runtime processing interval observed for that work
Queue      = ordered inputs whose work has not started
Foreground = priority projection of active work, queue, and pending action
```

- **Run** is the logical-work projection rooted at the AgentInput that started
  it. Its stable ID is derived from or permanently mapped to that root input.
  Steers, ModelSteps, tools, pending actions, interruption, and terminal facts
  carrying the same causal root determine Run content and state.
- **Execution** is the operational projection of runtime start/finish and
  recovery facts. Existing Execution IDs remain useful for tracing and actor
  correlation, but Execution owns no second product lifecycle.
- **Turn** is the host product projection for user interaction. A queued Turn
  derives from its pending user input; a started Turn derives from the
  associated logical work. It may keep a stable ID for navigation and
  compatibility without becoming durable state authority.
- **Queue** and **Agent foreground** are materialized projections, not mutable
  aggregates.

Derived does not mean ephemeral or identity-free. hostd may durably materialize
derived read models for fast queries, and commands may address their stable
IDs. Their state must nevertheless be reproducible from primitive facts.

| Concept | Semantic layer | Physical representation may remain |
|---|---|---|
| AgentInstance | Primitive | durable identity, actor, lifecycle facts, snapshots |
| AgentInput | Primitive | journal record, inbox/queue cache, protocol DTO |
| ModelStep | Primitive | atomic journal relation and trajectory correlation |
| Run | Derived | stable Run ID, Run records, active-Run maps, read models |
| Execution | Derived | Execution actor, journal-backed `StoredExecution`, recovery indexes |
| Turn | Derived | stable Turn ID, process-local host `TurnRecord`, usage and UI projections |
| Queue/Foreground | Derived | materialized read-model fields and runtime caches |

Keeping a derived record or cache is not a model violation. Allowing it to
accept independent mutations that cannot be reproduced from lower-layer facts
is the violation.

### Causal root

Every fact participating in logical work carries or deterministically resolves
one `root_input_id`. The starting AgentInput is its own root. A steer captures
the active root at admission. ModelSteps, tool activity, pending actions, and
terminal outcomes retain that root.

`RunId` may remain an opaque protocol type, but new Runs derive it
deterministically from `root_input_id` or store a single immutable mapping at
admission. There is no independently mutable Run record.

### Authority

- orchd serializes AgentInput admission and operates model/tool work.
- hostd's append-only journal is authoritative for admitted inputs, causal
  relations, atomic ModelStep commits, and terminal facts.
- hostd materializes Run, Execution, Turn, Queue, and Foreground views.
- clients consume host projections and never merge private lifecycle guesses.
- accepted input is durably committed before acceptance is reported.

### Control vocabulary

Canonical mutation targets are primitive identities:

- submit an AgentInput to an AgentInstance;
- cancel a pending AgentInput;
- interrupt the current work of an AgentInstance.

Run-, Execution-, and Turn-addressed commands are resolvers over derived
views. Before mutation they resolve to the current AgentInstance, root input,
pending input, or runtime cancellation handle. Resolution is generation-safe:
an old derived ID cannot target later work.

## Consequences

- Storage has one fact vocabulary rather than Turn, Run, and Execution state
  machines that must agree.
- Existing Run maps, journal-backed execution projections, and process-local
  host Turn projections may remain and be evolved in place.
- Run remains the primary logical-work view used by product and diagnostics,
  but not a write aggregate.
- Existing Execution facts remain valid inputs to the Execution projection;
  this refactor adds no multi-attempt product model.
- UserTurn remains a first-class UI object derived from the same facts as Run.
- Queueing and steering become AgentInput dispositions with causal-root
  bindings.
- The migration adds primitive facts and shadow projections before removing
  old authorities; existing commands and records remain compatibility inputs.
- ADR-025 remains authoritative for atomic ModelStep commits and distinct
  correlation IDs. Its hierarchy as a set of independently authoritative
  lifecycle entities is superseded.

## Rejected alternatives

- **Treat every stable scope as a primitive:** derived scopes can be stable
  and addressable; duplicating their state still creates competing truth.
- **Keep Turn as the universal root:** detached work makes it false, while
  synthetic Turns corrupt user interaction history.
- **Rename Run to WorkEpoch and persist it:** this preserves the same duplicate
  aggregate under a different name.
- **Infer work only from transcript adjacency:** pending input, tools,
  interruption, concurrency, and crash recovery require explicit causal facts.
