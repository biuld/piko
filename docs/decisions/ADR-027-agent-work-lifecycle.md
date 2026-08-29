# ADR-027: Session, Agent, AgentInput, ModelStep

> Status: proposed
> Date: 2026-08-30
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
primitive. We need invariant grains, one stimulus type, and derived queries
that cannot accept independent mutations.

## Decision

The invariant grains are Session, AgentInstance, and ModelStep. AgentInput is
the stimulus between Agent and ModelStep. There is no abstraction between
Session and Agent. Between Agent and ModelStep the only mid-granularity
derived view is the causal closure of a root AgentInput. Turn, Run, and
Execution are not identities.

### Invariants and stimulus

```text
Session
└── AgentInstance
    ├── AgentInput
    └── ModelStep
        └── Message / Thought / ToolCall / ToolResult
```

- **Session** is the durable journal boundary.
- **AgentInstance** is the durable identity and admission serialization
  boundary for one collaborator.
- **AgentInput** is an immutable, idempotent stimulus. It can remain pending,
  start work (`applied_as_root`), steer a later ModelStep, or be cancelled
  before application. When applied as root, its `input_id` is the work
  identity (`root_input_id`).
- **ModelStep** is the atomic model request/response boundary. It relates
  assistant output and tool declarations and is independently required for
  recovery and reliable observation.

Lifecycle facts connect these primitives: input admitted / disposition
changed, step committed, tool and pending-action facts, processing
interrupted / terminal outcome on the root input.

### Derived queries, not identities

```text
active work    = unfinished applied_as_root input + facts sharing that root
queue          = pending_follow_up inputs in admission order
pending steers = pending_steer inputs bound to the active root
foreground     = priority projection of active work, queue, and pending action
```

Steer, interrupt isolation, and follow-up advancement need this mid grain
because a ModelStep may not exist yet and Agent is too coarse to bind a
generation. The grain does not get a second ID.

| Concept | Semantic layer | Physical representation |
|---|---|---|
| Session | Invariant | journal, `session.json` identity |
| AgentInstance | Invariant | durable identity, actor, lifecycle facts, snapshots |
| AgentInput | Stimulus / work identity when root | journal record, protocol DTO |
| ModelStep | Invariant | atomic journal relation |
| Active work / queue / foreground | Derived query | `AgentWorkSnapshot` fields and indexes |

Keeping a derived snapshot is not a model violation. Allowing it to accept
independent mutations, or giving it `turn_id` / `run_id` / `execution_id`,
is the violation.

### Causal root

Every fact participating in a bout of work carries or deterministically
resolves one `root_input_id`. The starting AgentInput is its own root. A
steer captures the active root at admission. ModelSteps, tool activity,
pending actions, and terminal outcomes retain that root.

### Authority

- orchd serializes AgentInput admission and operates model/tool work. A live
  actor may exist internally, keyed only by `root_input_id`.
- hostd's append-only journal is authoritative for admitted inputs, causal
  relations, atomic ModelStep commits, and terminal facts on the root.
- hostd materializes queue, foreground, and active-work views.
- the TUI consumes host projections and never merges private lifecycle
  guesses. Desktop is out of scope for this decision's cutover.
- accepted input is durably committed before acceptance is reported.

### Control vocabulary

Canonical mutation targets are primitive identities:

- submit an AgentInput to an AgentInstance;
- cancel a pending AgentInput;
- interrupt the current work of an AgentInstance (the active root).

There are no Turn-, Run-, or Execution-addressed commands.

## Consequences

- Storage has one fact vocabulary. Remaining F-51/D-68 slices delete leftover
  Turn, Run, and Execution types, IDs, maps, commands, and projections.
- Queueing and steering are AgentInput dispositions with causal-root
  bindings.
- The cutover publishes primitive facts and `AgentWorkSnapshot` as the live
  contract. There is no dual-write or compatibility-adapter period. F-51/D-68
  implement this for the TUI only.
- Trajectory, prompt assembly, and usage rekey to `root_input_id`.
- ADR-025 remains authoritative for atomic ModelStep commits. Its hierarchy
  as a set of independently authoritative lifecycle entities is superseded.

## Rejected alternatives

- **Treat every stable scope as a primitive:** duplicating their state still
  creates competing truth.
- **Keep Turn as the universal root:** detached work makes it false.
- **Keep Run as a derived identity mapped from root input:** the mapping is
  the leftover hierarchy; identity is already `input_id`.
- **Keep Execution as the operational scope:** product 1:1 with the root
  makes a second ID useless; an internal actor keyed by the root is enough.
- **Infer work only from transcript adjacency:** pending input, tools,
  interruption, concurrency, and crash recovery require explicit causal
  facts.
- **Keep old submit/steer/cancel types as compatibility adapters:** dual-write
  recreates competing answers. F-51 deletes those types and updates the TUI
  only.
