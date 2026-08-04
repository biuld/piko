# D-25: Inter-agent completion fragments

> Status: accepted
> Implements: [F-20](../features/F-20-inter-agent-fragments.md)

## Goal

When a recipient agent starts a run with unread detached inbox reports, inject
one retained, idempotent completion Context message per report into the durable
transcript chain before the run input (after any world-state Context), without
consuming the inbox or starting turns.

## Constraints and non-goals

- Durability before activation: completion commits sit on the same
  PreparedExecution commit path as world-state / input.
- Linear parent chain (`parent_message_id == head`) must never be violated.
- No mid-run mutation of a concurrent parent execution’s head.
- Non-goals: status-only notifications, MESSAGE/NEW_TASK envelopes,
  `trigger_turn`, frozen-prompt blocks for completions.

## Proposed design

### Ownership

| Concern | Owner |
|---|---|
| Content format + stable message ids | `piko-protocol` (pure helpers next to `turn_abort_marker` / `world_state_message_id`) |
| Which inbox items need a fragment | `piko-orchd` `AgentActor` at run start (local inbox + transcript scan) |
| Durable commit order | `piko-orchd` `prepare_execution` / `PreparedExecution::commit_input` |
| Host durable store | Existing `ExecutionCommitPort` / `commit_message` (no new hostd API) |

hostd stays authoritative for shards; orchd only appends via the commit port
it already uses for every run.

### Content and identity (protocol)

```rust
// messages.rs
pub fn agent_completion_message_id(report_id: &str) -> String {
    format!("agent.completion/{report_id}")
}

pub const AGENT_COMPLETION_SOURCE_KIND: &str = "agent.completion";
pub const AGENT_COMPLETION_SUMMARY_MAX_CHARS: usize = 4_000;

pub fn agent_completion_context_message(report: &AgentRunReport) -> Message { /* ... */ }
```

Body (fixed key order, omit empty summary):

```text
inter-agent completion:
source_agent_instance_id: <report.agent_instance_id>
report_id: <report.report_id>
outcome: succeeded|failed|cancelled
summary: <truncated text>
```

`PromptSource { kind: "agent.completion", locator: report_id }`,
`trust: Trusted`, `Message::Context`.

Truncation: character-safe prefix of `AGENT_COMPLETION_SUMMARY_MAX_CHARS` with
a trailing `…` when cut.

### Selection at agent run start (`AgentActor`)

Before `prepare_execution`, build `pending_completions: Vec<(MessageId, Message)>`:

1. Collect inbox items with `consumed_at.is_none()`.
2. Sort by `(committed_at, report_id)`.
3. Drop items already represented in the in-memory transcript as a Context
   message with `source.kind == agent.completion` and
   `source.locator == report_id`.
4. Format remaining reports with `agent_completion_context_message`.

### Durable chain (`StartExecutionRequest` + `prepare_execution`)

```rust
// execution.rs
pub struct StartExecutionRequest {
    // ...
    pub world_state: Option<Message>,
    /// Unread detached completions; committed after world_state, before input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inter_agent_completions: Vec<Message>,
    pub input_message_id: MessageId,
    // ...
}
```

Each completion’s message id is `agent_completion_message_id(report_id)`,
recovered by scanning the Context source locator when building commits:

```text
chain_parent = context.head_message_id
if world_state:
  commit world_state with parent chain_parent; chain_parent = world_state_id
for each completion message in order:
  id = agent_completion_message_id(source.locator)
  commit with parent chain_parent; chain_parent = id
input_commit.parent = chain_parent
```

`PreparedExecution` stores `completion_commits: Vec<MessageCommit>` and
`commit_input` persists: world_state → completions… → input.

### In-memory transcript (`ExecutionActor::new`)

After optional world_state push, push each completion Message in the same
order, then the user input. `head_message_id` remains the **input** id (same
as world-state behavior: first live head after commits is input).

### Agent transcript bookkeeping

On successful `commit_input` / activation, `AgentActor` already takes the live
execution’s transcript projection at terminal; completions appear in recovered
`messages` on later runs, so the source scan remains correct after resume.

If input commit fails after partial completion commits, hostd may retain the
completion messages. The next run’s selection skips them via the transcript/
durable content check when recovery loads full messages. Live actor after a
failed start keeps the prior in-memory transcript until recovery—retry path
must re-load or treat durable head as source of truth. Existing
`RunStartupScope` failure handling: document that durable completion
commits stick (like world-state) and the next successful run is idempotent
because message ids match.

### Out of path for attached spawn

`spawn_agent` waits on the tool path and does not `CommitReport` to the parent
inbox, so selection never sees a report and injects nothing.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `agent_completion_message_id`, `agent_completion_context_message`, formatting helpers; `StartExecutionRequest.inter_agent_completions` |
| `piko-orchd` | AgentActor pending selection; prepare_execution chain; ExecutionActor push; unit + multi-agent integration tests |
| `piko-hostd` | none (existing message commit / recovery) |
| `piko-llmd` | none (Context already user-role data render) |

## Reusable infrastructure

- No `island-rs` change required.

## Failure and cancellation

- Completion commit failure: run start fails with `PersistenceFailed` (same as
  world-state/input).
- Cancellation after partial completion commits: markers and next-run
  selection stay consistent via stable message ids.
- Permanent delivery failure: no inbox, no fragment.

## Verification

- Unit tests for format, truncation, outcome lines, message id stability.
- Integration: detached child → inbox → parent run → MessageCommit chain shows
  completion before input; inbox unread; collect still works; second run no
  duplicate; collect-first path skips inject.
- Differential notes in `docs/verification/V-25-inter-agent-fragments.md`.

## Alternatives considered

| Alternative | Why not |
|---|---|
| Immediate parent inject on `CommitReport` | Races parent mid-run head; needs complex handoff into live ExecutionActor |
| Frozen prompt block only | Not retained across residual turns; diverges from F-04 Context model |
| Auto-consume on inject | Breaks F-10 explicit collect semantics |
| Expand CommitReport to write the Context | Couples delivery to parent head while parent may be busy |

## Rollout

1. Protocol helpers + request field.
2. prepare_execution / ExecutionActor chain.
3. AgentActor selection wiring.
4. Tests + V-25 + roadmap/index updates.
