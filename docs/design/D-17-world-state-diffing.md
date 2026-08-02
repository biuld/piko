# D-17: World-state diffing across turns

> Status: accepted
> Implements: [F-04](../features/F-04-context-management.md) (slice 2)

## Goal

Move the F-03 `state.run` world-state facts out of the frozen per-run prompt
into a retained transcript Context message that is injected once in full and
then diffed across runs, with hostd owning the durable baseline and clearing
it on compaction.

## Constraints and non-goals

- hostd stays authoritative for durable, user-visible state (ADR-002):
  facts, baseline, and the full/diff decision live in hostd; orchd only
  injects and commits the message.
- The durable AgentInstance transcript is a strict linear parent chain
  (`commit_message_under_lock` requires `parent == head`), so the
  world-state commit must precede the input commit.
- No `llmd` change: `Message::Context` is already rendered as a user-role
  data-only message.
- Non-goals: diffing environment blocks, subagent world-state, rollout
  patch records, token-budget fragments.

## Proposed design

### Fact model (hostd `domain/prompts/world_state.rs`, new)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunKind { Initial, Continuation }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldStateFacts {
    pub session_id: Option<String>,
    pub agent_instance_id: Option<String>,
    pub operation_id: Option<String>,
    pub run_kind: RunKind,
    pub model: Option<String>,
}
```

Pure functions on the fact set:

- `world_state_full_content(&WorldStateFacts) -> Option<String>` — the exact
  F-03 `state.run` line format (fixed key order, absent facts omitted).
- `world_state_diff_content(previous, current) -> Option<String>` — header
  line `world-state changed since the previous run:` plus one
  `fact: value` line per changed fact in fixed order; removals render
  `fact: <unset>`; `None` when nothing changed.
- `world_state_context_message(content: String) -> Message` — a
  `Message::Context` with `trust: Trusted`, source
  `PromptSource::new("run-state", "hostd/session")`, and a commit
  timestamp.

### hostd turn assembly (`application/turns/submit.rs`)

On the accepted root turn, after computing `continuation` and `model`
(existing code):

```text
facts = WorldStateFacts { session_id, agent_instance_id, operation_id: turn_id,
                          run_kind: continuation? Continuation : Initial, model }
previous = state.record_world_state(&session_id, facts.clone())?   // returns old, stores new
message  = previous.map(|p| world_state_diff_content(&p, &facts))
                   .unwrap_or_else(|| world_state_full_content(&facts))
                   .map(world_state_context_message)
prompt_resources.world_state = message
storage.set_world_state_baseline(&session_dir, Some(&facts))        // durable, best-effort
```

`record_world_state` mirrors `record_turn_model`: in-memory
`SessionState.world_state_baseline` returns the previous value and is
replaced by the current facts. The durable write mirrors `set_last_model`
(`manifest.world_state_baseline = baseline.cloned()` under the session IO
lock).

### Protocol (`piko-protocol`)

- `PromptResourceSnapshot` gains
  `world_state: Option<Message>` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]`). It is the host-owned carrier for the run's
  world-state message; the prompt assembler ignores it.
- `StartExecutionRequest` gains the same `world_state: Option<Message>`.
- `messages.rs` gains `world_state_message_id(execution_id) ->
  "{execution_id}/world_state"` next to `turn_abort_marker_message_id`.
- `AGENT_RUN_PROMPT_ASSEMBLY_VERSION` bumps `3 → 4` (block catalog no
  longer contains `state.run`).

### orchd run-start injection (`runtime/execution`)

- `start_execution_from` copies
  `request.prompt_resources.as_ref().and_then(|r| r.world_state.clone())`
  into `StartExecutionRequest`.
- `prepare_execution` builds an optional `world_state_commit`:
  `MessageCommit { message_id: world_state_message_id(execution_id),
  parent_message_id: context.head_message_id, message }`, and changes the
  input commit's parent to the world-state message id when present (keeps
  the durable chain linear: head → world-state → input).
- `PreparedExecution.commit_input` commits the world-state message first,
  then the input; `PreparedExecution` carries
  `world_state_commit: Option<MessageCommit>`.
- `ExecutionActor::new` pushes the world-state `Message` into the transcript
  before pushing the input; `head_message_id` stays the input message id.
  The in-memory order matches the durable order.

### Compaction invalidation (`application/compaction.rs`)

In `run_compact_rewrite`, after `compacted = true`:

```text
storage.set_world_state_baseline(&path, None)      // durable clear
session.world_state_baseline = None                 // in-memory clear
```

This covers auto Summarize, manual `session.compact`, and the
`new_context_window` tool callback — all funnel through
`compact_session_if_needed` → `run_compact_rewrite`. The next run therefore
re-injects the full snapshot.

### Prompt catalog change

`snapshot_prompt_resources` drops the `state.run` block emission, and
`PromptSnapshotOptions` drops the now-unused `session_id`,
`agent_instance_id`, `operation_id`, and `continuation` fields (keeps
`model` / `previous_model` for `context.model-switch`).

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `PromptResourceSnapshot.world_state`; `StartExecutionRequest.world_state`; `world_state_message_id`; assembly version 3 → 4 |
| `piko-hostd` | new `domain/prompts/world_state.rs`; `SessionState.world_state_baseline` + `record_world_state`; `submit.rs` wiring + durable persist; manifest field + `set_world_state_baseline`; compaction clearing; `state.run` block removal |
| `piko-orchd` | run-start world-state commit + transcript push; commit ordering |
| `piko-llmd` | none (Context messages already render) |
| `piko-sandbox` | none |

## Reusable infrastructure

- No `island-rs` change required.

## Failure and cancellation

- **Startup failure after world-state commit**: the durable transcript
  keeps the message; the run is rolled back (reservation only). The next
  run diffs against the already-recorded baseline, which is consistent
  because the baseline and its message were committed for the same run.
- **Dispatch failure before orchd commit** (hostd side): the in-memory +
  durable baseline still advance, but no message is committed; the next
  run's diff is computed against a baseline with no retained message.
  Residual gap is limited to facts that changed in the failed run (only
  `operation_id` in the common case; model changes are additionally
  reported by `context.model-switch`). Documented, low risk.
- **Cancelled run**: the abort marker (F-01) is appended after the
  world-state + input messages; the model sees the run's world-state lines
  and the interruption marker.
- **Compaction summarization failure**: `run_compact_rewrite` returns before
  the rewrite, so the baseline is untouched — full snapshots stay retained.

## Verification

- Hostd unit: `world_state_full_content` byte-matches the F-03 block
  content; `world_state_diff_content` emits changed facts in order,
  `<unset>` for removals, and `None` for identical fact sets.
- Hostd storage: manifest round-trips `world_state_baseline`;
  `set_world_state_baseline(None)` clears it.
- Hostd state: `record_world_state` returns previous and records current.
- Orchd unit: world-state message precedes input in the actor transcript;
  input commit parent is the world-state id when present.
- Integration: two-turn session → full then diff messages retained in the
  session transcript before each user message; compaction clears the
  baseline and the next run re-injects full.
- Regression: `semantic_prefix_digest` unchanged by environment blocks;
  assembly version 4.

## Alternatives considered

- **Hostd-only diff of the frozen `state.run` block** (no retention).
  Rejected: continuation runs would lose stable identity facts because the
  frozen prompt is rebuilt per run and never retained in the transcript;
  also near-no-op for the current five-fact block.
- **Commit the world-state message from hostd before dispatch**. Rejected:
  would require hostd to know orchd's pre-run head and pre-commit into the
  shard, splitting the commit path; letting orchd commit through the
  standard `MessageCommitScope` keeps one commit mechanism.
- **JSON merge-patch (RFC 7386) like codex-rs**. Rejected (adapted): the
  piko fact set is flat and line-rendered; a line diff is smaller and
  matches the existing block format.

## Rollout

1. Protocol DTOs + version bump + message-id helper.
2. Hostd fact model, baseline state, submit wiring, storage, compaction
   clearing, `state.run` removal.
3. Orchd run-start injection and commit ordering.
4. Tests + verification evidence (V-17) + roadmap/digest/F-03 updates.
