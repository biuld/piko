# D-26: Session branch-point fork

> Status: implemented
> Implements: [F-09](../features/F-09-session-persistence.md) slice 1

## Goal

Make `SessionFork { entry_id: Some(_) }` create a new durable schema-v3
session whose history is the single ancestor path through that entry,
without mutating the source and without partial writes on failure.

## Constraints and non-goals

- hostd owns durability; orchd is not involved in fork.
- Schema stays v3; no on-disk format migration.
- Reuse `active_branch_entries` lineage walk (same parent graph as
  compaction / navigate summaries).
- Non-goals: in-session navigate changes, list paging, shared storage
  between source and fork, restoring live multi-agent queues.

## Proposed design

### Command path

```text
SessionFork { session_id, entry_id }
  → HostApp::apply_session_fork
      → reject if source has active_turns
      → SessionRepository::fork(source_id, source_dir, entry_id)
          → JsonlSessionRepository::fork
              → entry_id None  → fork_full (existing + baseline clear)
              → entry_id Some  → fork_at_entry (new)
      → insert forked session + session_open_response + reconcile
```

### Active turns

Before any IO beyond resolving the source path, if the hydrated source has
non-empty `active_turns`, return `ProtocolError::ActiveTurnExists` (same
guard as `apply_session_navigate`).

### Full clone adjustment

`fork_to` (and thus full clone) **clears** after copy:

- `world_state_baseline = None`
- `agent_inbox = []`
- `agent_input_queue = []`
- `agent_executions = {}`

So both fork modes leave a static history without phantom running work and
without a mismatched F-04 baseline (PRD open question closed: apply to both).

### Branch-point algorithm (`fork_at_entry`)

1. `load_session_dir(source)` → `PersistedSession` (manifest + projected
   entries from all agent shards, same as open).
2. If no entry with `id == entry_id`, return `Invalid` with a stable message
   (`unknown tree entry: …`).
3. `retained = active_branch_entries(&state.entries, Some(entry_id))`.
4. Build sets:
   - `kept_entry_ids` from retained
   - `kept_message_ids` from retained `Message` / `ToolCall` ids
   - `kept_agent_ids` from those messages’ `agent_instance_id`, always
     unioned with `root_agent_instance_id` when present
5. Create destination directory layout (same naming as full clone:
   `{created_at}_{new_uuid}` under the source cwd bucket).
6. Write destination manifest:
   - new `session_id`, `created_at` / `updated_at`
   - `entries` = source **manifest** entries whose id is in `kept_entry_ids`
     (shard-only messages are not duplicated into `manifest.entries`)
   - `agents` filtered to `kept_agent_ids`; drop `latest_report` (static cut)
   - `current_leaf_id = Some(entry_id)`
   - `selected_agent_instance_id` = root if still present, else first kept
   - cleared baseline / inbox / queue / executions as above
   - keep `last_model`, `cwd`, `name` (optional: suffix ` (fork)` deferred)
7. For each agent in `kept_agent_ids`:
   - load source shard records
   - rewrite header `session_id` to the new id
   - copy `Message` records whose `id` is in `kept_message_ids`, preserving
     parent links (all parents on path are also kept)
8. `load_session_dir(dest)` and return `PersistedSession`.

### Failure hygiene

Create destination directory only after validating `entry_id`. On any
subsequent IO error, best-effort `remove_dir_all` of the new path so no
orphan half-fork remains.

### Layering

| Piece | Location |
|---|---|
| Domain lineage | existing `domain/compaction/tree::active_branch_entries` |
| Storage filter + write | `infra/storage/jsonl_repository/fork_import.rs` (+ small helper) |
| Active-turn guard | `application/sessions/mutate.rs` `apply_session_fork` |
| Wire command | unchanged `Command::SessionFork` |

No protocol DTO changes.

### Tests

Unit / integration under `piko-hostd`:

1. Full clone still clones full path length; baseline empty on dest.
2. Linear history + branch-point at mid entry → dest entry/message count
   equals path length; source unchanged.
3. Session with a sibling branch after navigate → fork at early entry has no
   sibling descendants.
4. Unknown `entry_id` fails; dest dir not left behind.
5. Active turn on source fails fork before write.

## Alternatives considered

| Option | Why not |
|---|---|
| Full clone then truncate dest | Extra IO and easier to leave inconsistent agents |
| Soft link / COW of shards | Couples sessions; delete source unsafe |
| Truncate in place | Violates “source unchanged” |

## Verification

See [V-26](../verification/V-26-session-branch-point-fork.md).
