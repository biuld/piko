# V-17: F-04 world-state-diffing slice acceptance evidence

> Date: 2026-08-03
> Fixture: `piko-hostd` unit + integration tests (`world_state.rs`,
> `model_continuity.rs`, `resources.rs`, `compaction_reconcile.rs`,
> `agent_directed_chat.rs`), `piko-orchd` execution tests
> Environment: macOS, `cargo test --workspace` (network-capable execution),
> `cargo clippy --workspace --all-targets -- -D warnings`

## Reproduction

```bash
cargo test -p piko-hostd --lib domain::prompts::world_state
cargo test -p piko-hostd --lib sessions::tests::record_world_state_returns_previous_facts_and_tracks_baseline
cargo test -p piko-hostd --test model_continuity
cargo test -p piko-hostd --test resources
cargo test -p piko-hostd --test compaction_reconcile
cargo test -p piko-hostd --test agent_directed_chat
cargo test -p piko-orchd --lib runtime::execution
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The hostd integration tests drive the real `ChatSubmit` → snapshot →
durable-baseline path with capturing runners over a real JSONL session
repository; the orchd test drives run-start commit ordering.

## Result

All F-04 slice 2 acceptance criteria pass:

- **Full injection on a fresh session**: run 1's `world_state` message
  contains `session_id`, `agent_instance_id`, `operation_id`, `run_kind:
  initial`, and `model` in fixed order (unit + `model_continuity`
  integration).
- **Diff on continuation**: run 2's `world_state` starts with
  `world-state changed since the previous run:` and lists only the changed
  facts (`operation_id`, `run_kind: continuation`); no `session_id`,
  `agent_instance_id`, or unchanged `model` line, and those facts remain
  visible through the retained full snapshot from run 1.
- **Identical facts → no message**: `world_state_diff_content` returns
  `None` for an unchanged fact set (unit).
- **Removal marker**: a fact that becomes unavailable renders
  `fact: <unset>` (unit).
- **Durable baseline across restart**: `session.json` round-trips
  `worldStateBaseline`; a fresh host loading the same directory restores it
  and continues diffing (integration).
- **Compaction clears the baseline**: after a `NewContextWindow` compact the
  durable baseline is `None` and the next run re-injects the full snapshot
  (`compaction_reconcile` integration).
- **Prompt shape**: the frozen prompt has no `state.run` block;
  `AGENT_RUN_PROMPT_ASSEMBLY_VERSION` is `4`; environment/model-switch
  blocks stay RunDynamic and never change `semantic_prefix_digest`, while
  project-context changes still do (`resources.rs` regression guard).
- **Linear durable chain**: the world-state commit precedes the input commit
  (`parent = head`, input parent = world-state id), and the actor transcript
  is ordered world-state → input (`piko-orchd` execution test).
- **Child runs unaffected**: direct child-agent chats keep two commits
  (input + assistant) — no world-state injected (`agent_directed_chat`).
- **Differential validation**: the full → diff sequence mirrors codex-rs
  `update_world_state` full-then-patch behavior at the message level
  (two-turn integration evidence above).

## Invariants

- hostd owns the facts, baseline, and full/diff decision; orchd only injects
  and commits the retained Context message (ADR-002 hostd-authoritative
  invariant).
- The durable transcript chain stays linear (head → world-state → input).
- A rewritten transcript (compaction) always invalidates the baseline, so a
  diff never references a lost full snapshot.
- World-state messages are data-only Context (trust `Trusted`, source
  `run-state`/`hostd/session`, authority None); they never carry instruction
  authority.
- `cargo clippy --workspace --all-targets -- -D warnings` is clean and
  `cargo test --workspace` passes.
