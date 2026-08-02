# D-10: Multi-agent v2 collaboration tools

> Status: accepted
> Implements: [F-10](../features/F-10-multi-agent.md)
> Decisions: product decisions live in the F-10 PRD

## Goal

Deliver the four v2 collaboration tools (`followup_task`, `interrupt_agent`,
`list_agents`, `wait_agent`) on the existing multi-agent tool provider, backed
by a session-scoped mailbox notification lane.

## Constraints and non-goals

- No new durable state and no schema-v3 changes: the notification lane is
  best-effort in-memory; durability stays on the existing commit paths.
- **Durability before visibility**: each event fires only after the
  corresponding durable commit (`InputQueued`, `CommitReport`,
  `RunTerminal`) succeeded.
- `wait_agent` is observational: it never writes, never consumes inbox items,
  and is retry-safe.
- File sizes stay under the 500-line ceiling; changes are additive and do not
  restructure the actor.
- No changes to hostd, llmd, sandbox, or the protocol wire layout of existing
  types.

## Proposed design

### Protocol DTOs (`piko-protocol`)

```rust
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentMailboxEvent {
    InboxReport { agent_instance_id: AgentInstanceId, report_id: String, source_agent_instance_id: AgentInstanceId },
    RunFinished { agent_instance_id: AgentInstanceId, report_id: String },
    InputQueued { agent_instance_id: AgentInstanceId, request_id: String },
}

pub struct MailboxWaitRequest {
    pub session_id: String,
    pub caller_agent_instance_id: Option<AgentInstanceId>,
    pub timeout_ms: u64,
    pub agent_instance_id: Option<AgentInstanceId>, // filter
}

pub struct MailboxWaitSummary {
    pub timed_out: bool,
    pub event: Option<AgentMailboxEvent>,
    pub agents: Vec<AgentSnapshot>, // tree-sorted snapshot list
}
```

`AgentMailboxEvent` has `fn agent_instance_id(&self) -> &str` so the filter
and consumers share one access path.

### Comm catalog (`piko-comms`)

Add `AgentMailboxEvent` as a `BroadcastContract`:

- id `orchd.agent.mailbox_event`, kind `Observation`, scope `Session`,
  delivery `BestEffort`, capacity `Bounded(64)`, overflow `DropNewest`,
  producer `AgentActor`, consumer `AgentRuntime::wait_agent_mailbox`.

### Session notification lane (`SessionAgentScope`)

`SessionAgentScope` owns `mailbox_events:
BroadcastSender<AgentMailboxEventContract, AgentMailboxEvent>` created in
`SessionAgentScope::new`, with a `mailbox_events()` accessor. The actor
reaches it through its existing `Weak<SessionAgentScope>`; the runtime
subscribes through `scope.mailbox_events().subscribe()`.

### Publication points (`AgentActor`)

`AgentActor::publish_mailbox_event(event)` upgrades the weak scope and sends;
send failure (session torn down) is ignored. It is called at:

1. `enqueue_follow_up` — after the `InputQueued` commit succeeds and the item
   is enqueued: `InputQueued { agent_instance_id, request_id }`.
2. `AgentCommand::InboxReport` handler — after the item is pushed (only when
   it was not already present): `InboxReport { agent_instance_id, report_id,
   source_agent_instance_id }`.
3. `try_commit_terminal`, `Committed` branch — after `publish_snapshot`:
   `RunFinished { agent_instance_id, report_id }`.

### Runtime wait API (`AgentRuntimeApi` + `AgentRuntime`)

New trait method:

```rust
async fn wait_agent_mailbox(
    &self,
    request: piko_protocol::MailboxWaitRequest,
) -> Result<piko_protocol::MailboxWaitSummary, AgentApiError>;
```

Implementation: resolve the session scope, optionally validate that the
caller agent exists, `subscribe()`, then `tokio::time::timeout` around
`recv()` with a loop that skips `Lagged` errors and applies the optional
single-agent filter. On timeout (or channel close) `timed_out: true`; on the
first matching event `timed_out: false`. The snapshot list is produced by the
existing tree-sorted `AgentRuntime::list_agents`.

### Tool provider (`MultiAgentToolProvider`)

Four new tool defs (Sequential execution mode, `Delegation` capability,
`Never` approval, consistent with the existing provider):

| Tool | Input schema | Runtime call |
|---|---|---|
| `followup_task` | `agent_instance_id` + `message` | `send_agent_input` with `delivery: FollowUp`; result `{agent_instance_id, disposition}` |
| `interrupt_agent` | `agent_instance_id` | pre-cancel snapshot + `cancel_agent_run`; `InvalidState` mapped to `{accepted: false, previous_activity}` |
| `list_agents` | none | `list_agents(session_id)`; result array with identity/lifecycle/activity/unread/latest summary |
| `wait_agent` | `timeout_ms` (required), `agent_instance_id` (optional) | `wait_agent_mailbox`; result `{timed_out, event, agents}` |

`wait_agent` also selects on `context.cancellation` so a cancelled parent turn
aborts the wait promptly (same pattern as attached spawn).

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `AgentMailboxEvent`, `MailboxWaitRequest`, `MailboxWaitSummary` |
| `piko-comms` | `AgentMailboxEvent` broadcast contract + catalog entry |
| `piko-orchd-api` | `wait_agent_mailbox` on `AgentRuntimeApi` |
| `piko-orchd` | scope lane, actor publication, runtime wait, provider tools |
| `piko-hostd` | none |
| `piko-llmd` | none |
| `piko-sandbox` | none |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Wait timeout: clean `timed_out` summary, no writes.
- Broadcast `Lagged`: skip and continue waiting.
- Session detach while waiting: scope is gone → `SessionNotAttached`.
- Parent-turn cancellation: `wait_agent` and attached-follow-up paths select on
  the tool cancellation token; a cancelled child run leaves durable abort
  markers per F-01.
- `interrupt_agent` on an idle target: benign `accepted: false` result.
- Event publication failures (weak scope dead): ignored; waiting callers time
  out rather than hang forever.

## Verification

- Integration tests in `packages/orchd/tests/agent_runtime_cases/multi_agent.rs`
  covering every acceptance criterion in F-10 (idle/busy follow-up, interrupt
  running/idle, tree-sorted list, wait event, wait timeout, wait filter).
- Differential validation: the v2 tool names and supervision semantics cite
  digest Block I evidence; piko-specific adaptation (mailbox lane, mandatory
  timeout, no consumption) is asserted by the tests.

## Alternatives considered

- **Polling-based `wait_agent`** (re-read snapshots until change): simpler but
  racy and wasteful; rejected in favor of a session notification lane that is
  also reusable by hostd later.
- **Reusing `send_agent_message` with a `delivery` argument**: keeps a smaller
  surface but diverges from the codex-rs v2 tool names that the digest
  tracks; rejected — v2 names are the stable supervision vocabulary.
- **Durable event log for mailbox events**: correct but heavy; rejected until
  a replay consumer exists (best-effort lane now, per PRD decision).

## Rollout

1. Protocol DTOs + comms catalog contract.
2. Scope lane + actor publication points.
3. Runtime `wait_agent_mailbox`.
4. Provider tools (`followup_task`, `interrupt_agent`, `list_agents`,
   `wait_agent`).
5. Integration tests + `docs/verification/V-10` evidence.
