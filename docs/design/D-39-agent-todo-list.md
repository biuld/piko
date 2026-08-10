# D-39: Agent todo list projection and dock strip

> Status: draft  
> Feature: [F-27](../features/F-27-agent-todo-list.md)

## Terminology

Follow F-27: product/docs/UI = **todo list** / **todo item**; protocol =
`TodoList` / `TodoItem` / `TodoListUpdated`; tools remain **`todo_*`**. Do not
use product **task list** or types `TaskList` / `TaskItem` (collides with F-01
background tasks and multi-agent wording).

## Goal

Make each AgentInstance’s **todo list** **host-authoritative product state**,
project it to clients, and render a **TUI dock strip** for the viewed agent.
Todo tools remain the agent write path; Timeline tool cards remain history only.

## Constraints

- **hostd** is authoritative for user-visible state (project AGENTS.md).
- orchd may **own runtime mutation** during tool execution but must **publish**
  results so host can persist and project.
- Do not put multi-line checklists in BottomBar.
- Do not invent client-only todo truth from replaying `todo_write` args as the
  long-term design (acceptable only as temporary fallback during migration).
- File size and layering rules: protocol DTOs in `piko-protocol`; no circular
  crate deps.

## Current baseline (as-is)

| Layer | Behavior |
|-------|----------|
| orchd `TodoProvider` | `HashMap<agent_id, Vec<Value>>`; `todo_write` replaces; `todo_read` returns |
| Persistence | **None** (process memory) |
| Host / protocol projection | **None** |
| TUI | Parses tool args/results in timeline presenters; optional force-body |

## Target architecture

```text
Agent tool todo_write/read
        │
        ▼
 orchd TodoProvider (runtime source of truth during process)
        │  publish on change (event or turn-result side channel)
        ▼
 hostd  persist + project (session snapshot + live events)
        │
        ▼
 protocol DTOs (TodoList / TodoItem per agent_instance_id)
        │
        ▼
 clients (TUI dock strip for viewed agent; other clients optional)
```

### Why not orchd-only forever?

Dock and resume require state after reconnect and across host-owned session
files. User-visible todo lists are product state → **host projection**.

### Why not TUI-derived from Timeline forever?

Transcript order and compaction make “latest todo_write” fragile; multi-agent
and extension fields need a typed projection.

## Protocol (normative serde)

Place in `piko-protocol` (e.g. `todo.rs` re-exported from lib). Match F-27.

```rust
// Illustrative — implementation may split modules but must preserve wire.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    /// Always string on the wire. Numbers from tools normalize to decimal string.
    pub id: String,
    pub status: TodoStatus,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    // no deny_unknown_fields — additive keys ignored by older clients
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoList {
    pub agent_instance_id: AgentInstanceId,
    pub items: Vec<TodoItem>,
    /// Epoch milliseconds.
    pub updated_at: i64,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoListUpdated {
    pub todo_list: TodoList,
}
```

**Normalize** tool payloads:

| Input | Output |
|-------|--------|
| `id`: number `1` | `"1"` |
| `id`: string `"a"` | `"a"` |
| missing `status` | default to `pending` (Rev B) |
| unknown `status` | reject write (item-indexed, actionable error) |
| missing / empty `content` (trim) | reject write |
| unknown item keys | keep on value map only if using `Value` intermediate; typed `TodoItem` drops unknown unless `#[serde(flatten)]` extras map is added later |

Rejection errors must be actionable (Rev B): they name the failing item
index, the offending field, and the accepted values so the model can correct
and retry the write in the same turn.

**Tool args** remain `{ "todos": [ ... ] }` (not `items`) for model stability.
Adapter: `todos` ↔ `TodoList.items`.

**Snapshot:** `todoLists: Vec<TodoList>` (or map).  
**Event:** e.g. session/server event carrying `TodoListUpdated`.

**Persist default:** on each agent’s durable record under session storage
(alongside agent instance metadata), field name e.g. `todoList`. Alternative:
session-level `todoListsByAgent` map. Prefer **per-agent field** so lifecycle
follows the agent.

## orchd

- Keep `TodoProvider` as execution-time store keyed by agent instance id string.
- Store **`Vec<TodoItem>`** (typed), not free-form `Value`, after normalize.
- On successful `todo_write`: validate → replace → bump revision/updatedAt via
  host path → publish to host (typed signal preferred).
- Return `{ "todos": [ normalized items... ] }` on write and read.
- On session hydrate: **seed** provider from host durable lists.
- Invalid item → tool error, no mutation.

## hostd

- **Persist** `TodoList` per agent with session durability (F-09).
- On todo mutation: write store → snapshot/event projection.
- Compaction / transcript rollup **must not** clear todo store.
- Snapshot includes all agents’ lists for client rehydrate.
- **Prompt assembly:** when building a frozen run prompt for agent A and
  feature `todo` is on:
  1. Load durable list for A.
  2. If `items` non-empty, append fragment `todo.list` (source e.g.
     `hostd/todo`, trust trusted) with a stable text render, e.g.:

     ```text
     Current todo list (3 items, 1 remaining):
     [x] 1: done work
     [~] 2: in progress work
     [ ] 3: still pending
     ```

  3. Ensure policy/instruction block (when todo tools are in catalog) includes
     drive copy (stable English), e.g.:

     > Maintain a todo list for multi-step work via todo_write / todo_read.
     > The current list is injected when non-empty. Prefer completing remaining
     > pending and in_progress items unless the user redirects; update the list
     > when the plan or progress changes so it stays an accurate lossy plan.

  4. Do **not** fold this into world-state full/diff (F-04); separate fragment
     so list updates do not pretend to be session_id/model fact diffs.

## Prompt fragment details

| Concern | Choice |
|---------|--------|
| When | Feature on + non-empty items for the **running** agent |
| Empty | Omit data fragment |
| Ordering | After identity/environment context, before volatile turn input (exact order with F-03 catalog is implementation; must be deterministic) |
| Cache | Non-stable / per-run context scope (list changes often) |
| Child agents | Own list only |
| Completeness | Full list every time (not diff-only), so the model always sees remaining work without reconstructing history |

## TUI

Normative client presentation lives in package docs (keep this section as a
cross-crate pointer + slice ordering only):

- **Prerequisite (Dock Stack infrastructure):**
  [dock-coexistence feature](../../packages/tui/docs/features/dock-coexistence.md) /
  [design](../../packages/tui/docs/design/dock-coexistence.md) — standalone TUI
  feature: band registry, offer/grant solver for non-resident plane bands.
  Todos is a **provider**, not a private `compose_plane` fixed height.
- Feature: [packages/tui/docs/features/todo-list.md](../../packages/tui/docs/features/todo-list.md)
- Design: [packages/tui/docs/design/todo-list.md](../../packages/tui/docs/design/todo-list.md)

### Plane (summary)

```text
Stream (grow)
Notice? (1)
Todos? (N, budgeted — e.g. min(items+1, cap))
Suggest?
Composer
```

Compose order: after Notice, before Suggest (attention > todos > completions >
editor). Empty → height 0. Details, caps, and module sketch → TUI design doc.

### State (summary)

- Subscribe to snapshot + `TodoListUpdated` (names TBD).
- Bind strip to **viewed** `agent_instance_id`.

### Presentation (summary)

- Shared checklist language with timeline `todo_*` presenters; strip is live
  truth, cards are history.
- Cap + overflow; remove force-open checklist bodies once strip ships.

### Migration fallback (optional short-lived)

- Until host events land, TUI may derive viewed-agent list from latest
  successful `todo_write` in that agent’s timeline **only as a bridge**. Must
  be deleted when projection ships so extension fields do not fork.

## Slices

| Slice | Deliverable |
|-------|-------------|
| **A** | Protocol `TodoStatus` / `TodoItem` / `TodoList` serde + normalize from tool JSON |
| **B** | Host durable store + snapshot; orch publish/seed; write returns normalized todos |
| **C** | Live `TodoListUpdated` |
| **D** | Prompt fragment `todo.list` + drive instruction (feature on) |
| **E** | TUI dock strip + plane budget; viewed-agent binding |
| **F** | Drop timeline force-body; tests; feature-off behavior |
| **G** (follow-on) | Richer item fields + strip density rules |

## Package impact

| Package | Change |
|---------|--------|
| `piko-protocol` | `TodoStatus` / `TodoItem` / `TodoList` / update event + snapshot fields |
| `piko-orchd` | Typed store; normalize; publish; seed; tool results |
| `piko-hostd` | Persist; project; **prompt fragment + drive instruction** |
| `piko-tui` | Dock strip; compose height; drop force-body |
| `piko-tui-layout` | None (plane stays product compose) |

## Failure and cancellation

- Failed `todo_write` → no list mutation, no event.
- Feature disabled → no tools; host exposes empty; strip hidden.
- Unknown future item fields → older clients show baseline fields only.

## Open design choices

1. Exact session file field path (per-agent `todoList` vs session map) — default
   per-agent field above.
2. Orch→host publish mechanism (dedicated event vs tool result envelope).
3. Strip default density (full vs summary).
4. Final English drive-instruction string (product copy).

## Non-goals

- Selectable table layout for todos in v1.
- Human multi-writer editing.
- BottomBar checklist.
