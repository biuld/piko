# Multi-Agent Runtime Model

> Status: current normative business and runtime model
> Technical base: [Agent Runtime Actor Design](single-agent-actor-runtime-design.md)

## 1. Core Model

The durable multi-agent structure is an AgentInstance tree inside one Session.

```text
Conversation Session
└─ AgentInstance tree
   ├─ root AgentInstance
   ├─ coder AgentInstance
   │  └─ reviewer AgentInstance
   └─ scout AgentInstance
```

Each AgentInstance is long-lived and may serve many inputs. Internally one input
may cause a short-lived Execution:

```text
AgentInstance
└─ 0..N sequential internal Executions
   └─ 1..N Model Steps
      └─ 0..N Tool Executions
```

Executions do not form the Agent hierarchy. There is no Execution tree or
Execution dependency graph.

## 2. Concepts

### 2.1 AgentSpec

An AgentSpec is immutable capability configuration: role, system prompt, model,
thinking level, and tool sets. hostd resolves it; AgentRuntime captures a
snapshot when creating an AgentInstance. `agent_spec_id` is never an address.

### 2.2 AgentInstance

```rust
struct AgentInstanceIdentity {
    session_id: SessionId,
    agent_instance_id: AgentInstanceId,
    agent_spec_id: AgentSpecId,
    parent_agent_instance_id: Option<AgentInstanceId>,
}
```

An AgentInstance owns a private transcript, inbox, follow-up queue, lifecycle,
historical reports, and at most one active internal run. It remains addressable
after a run and can be reused with accumulated context.

### 2.3 AgentInstance Tree

The tree is defined only by `parent_agent_instance_id`. It determines creation
authority, quotas, recovery order, and UI hierarchy. It is never inferred from
AgentSpec, display name, tool order, transcript path, or Execution order.

### 2.4 Agent Run and Report

“Agent run” is the Agent-level operation: accept input and produce a report.
Internally it uses an ExecutionActor, but callers never address that Execution.

An Agent report contains an opaque `report_id`, outcome, summary, usage, and
artifact references. `report_id` supports idempotent delivery and inbox
consumption; it is not an Execution address. Durable records may carry internal
Execution identity; Agent-facing DTOs and LLM results do not.

### 2.5 Interaction Turn

An Interaction Turn belongs to hostd and binds one root Agent run. Child Agent
runs created by tools do not create hostd Turns.

## 3. Cardinality and Addressing

```text
Session        1 ── N AgentInstance
AgentSpec      1 ── N AgentInstance
AgentInstance  1 ── 0..N child AgentInstance
AgentInstance  1 ── 0..N sequential internal runs
Turn           1 ── 1 root Agent run
Agent run      1 ── 1 terminal report
```

Every multi-agent operation addresses `session_id + agent_instance_id`. Never
route by AgentSpec ID, name, parent alone, creation order, or Execution ID.

## 4. Lifecycle and Activity

Lifecycle is durable:

```text
Open ↔ Closed
Open/Closed → Unavailable or Terminated
```

Activity is a live projection:

```text
Idle | Running | WaitingForApproval | Cancelling
```

A run outcome does not close the AgentInstance.

## 5. Runtime Ownership

```text
AgentRuntime
└─ SessionAgentScope
   └─ AgentInstance tree
      ├─ root AgentActor
      │  └─ internal ExecutionActor?
      └─ child AgentActor*
         └─ internal ExecutionActor?
```

AgentRuntime is the mandatory registry, policy boundary, router, and supervisor.
AgentActor exclusively mutates one AgentInstance. ExecutionActor owns one
short-lived model/tool loop. Arbitrary Actor mailbox exchange is forbidden.

## 6. Agent-Level Operations

The meaningful operations are:

- create an AgentInstance;
- run an Agent and await its report;
- send input and return after acceptance;
- send detached input whose report goes to an Agent inbox;
- steer or queue follow-up input;
- cancel an Agent's current run;
- inspect Agent status/tree/inbox;
- close or reopen an AgentInstance.

There is no public operation to start, wait, query, steer, or cancel an
Execution by ID.

## 7. LLM Tool Boundary

LLMs access multi-agent behavior only through typed AgentRuntime-backed tools.

### 7.1 `spawn_agent`

```text
create durable child
→ run_agent(child, prompt)
→ await Agent report
→ return Agent-only ToolResult
```

The parent tool future naturally waits. No child barrier or execution graph is
required.

### 7.2 `spawn_agent_detached`

```text
create durable child
→ atomically accept detached input + parent inbox delivery
→ return { agent_instance_id, status: accepted }
→ child continues independently
→ terminal report is committed to parent inbox
```

There is no second tracking call and no Execution ID in the result.

### 7.3 Reuse and status

`send_agent_message` targets an existing Agent and may start, steer, or queue a
run. `get_agent_status` returns lifecycle/activity without internal identity.
`collect_agent_reports` durably consumes inbox reports projected without
Execution identity.

## 8. Trusted Context and Authorization

ExecutionActor injects trusted Session, caller AgentInstance, internal run, and
tool-call identity. Internal run/tool identity is used only for audit and
idempotency. The model cannot forge Session, parent, caller, or origin.

Current policy permits self and direct parent/child control and rejects
unauthorized sibling/cross-Session messaging. AgentRuntime enforces depth and
Agent-count limits.

## 9. Private Transcript

Every AgentInstance owns a separate append-only shard:

```text
session.json
agents/<agent_instance_id>.jsonl
```

Children receive bounded input, never a mutable parent transcript reference.
Cross-Agent exchange uses messages and reports. Reuse preserves transcript
continuity; closing does not delete it.

## 10. Attached and Detached Completion

Attached completion registers an internal waiter in AgentActor before terminal
processing. Completed reports remain indexed internally so retries resolve the
correct historical result rather than merely observing the latest report.

Detached delivery is registered in the same mailbox command as input:

```text
commit source report
→ commit recipient inbox item
→ update recipient AgentActor
→ publish Agent projection
```

Durable inbox commit precedes live delivery. Report IDs are deterministic and
delivery is idempotent.

## 11. Persistence and Recovery

`session.json` stores root identity, Agent metadata/spec snapshots, lifecycle,
reports, inbox, and internal recovery metadata. Messages live in per-Agent
JSONL shards.

Recovery:

1. hostd loads schema-v3 state;
2. incomplete internal runs are marked interrupted;
3. AgentRuntime attaches one SessionAgentScope;
4. AgentActors restore private transcripts and inboxes;
5. recovered live activity starts Idle;
6. the stable root Agent identity is reused.

There is no migration from pre-v3 layouts.

## 12. Observation

Reliable committed events and lossy realtime deltas remain separate.

- committed messages/reports drive authoritative hostd projection;
- realtime deltas drive temporary TUI drafts;
- subscriber disconnect never affects an Agent run;
- internal Execution metadata may order/deduplicate events but is not an address.

## 13. Cancellation and Failure

- attached parent cancellation cancels the child Agent's current run;
- detached children survive parent cancellation by default;
- child failure returns a bounded Agent report;
- AgentActor panic makes the instance unavailable where commit permits;
- ExecutionActor failure terminates one run, not the AgentInstance;
- Session detach cancels and drains runs before Actor shutdown.

## 14. Non-Goals

- shared mutable parent/child transcripts;
- Execution trees or dependency graphs;
- arbitrary Actor-to-Actor messaging;
- routing by name or AgentSpec ID;
- automatic detached report injection into parent transcript;
- exposing Execution control to users or LLM tools;
- a Task/Work compatibility runtime.

## 15. Invariants

1. AgentInstance is the only user/LLM-visible multi-agent runtime identity.
2. Execution is internal to AgentRuntime.
3. Every attached AgentInstance has one owning AgentActor.
4. One AgentActor has at most one active internal run.
5. Agent creation is durable before routable.
6. Every terminal run produces a durable bounded report.
7. Detached delivery is durable and idempotent.
8. Private transcripts never share mutable state.
9. Multi-agent tools contain no Execution identity.
10. hostd remains authoritative for user-visible Session and Turn state.
