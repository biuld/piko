# Agent Todo List (TUI) Design

> Status: draft  
> Feature: [todo-list.md](../features/todo-list.md)  
> Parent: [F-27](../../../../docs/features/F-27-agent-todo-list.md),
> [D-39](../../../../docs/design/D-39-agent-todo-list.md)  
> **Prerequisite:** [dock-coexistence.md](./dock-coexistence.md)

## Goal

Wire host-projected `TodoList` into the chat shell as a **dock Todos strip**
for the viewed agent, keep Timeline as **history**, and share one checklist
presentation family without inventing client truth.

**Do not** add `Region::Todos` outside **Dock Stack**
([dock-coexistence](./dock-coexistence.md)): register band + offer builder +
paint under grant. Land Dock Stack solver (M1–M2) before or with the strip.

## Ownership

```text
host snapshot / TodoListUpdated
        │
        ▼
 AppState  Map<AgentInstanceId, TodoList>   (or session-scoped store)
        │  viewed agent id from session view
        ▼
 plane compose  todos_height = f(list, cap)
        │
        ▼
 features/todos (or dock todos) renderer  →  dock rows
```

| Concern | Owner |
|---------|--------|
| Wire types | `piko-protocol` (`TodoList`, `TodoItem`, …) — not TUI |
| Durable truth | hostd (F-27 / D-39) |
| Client store | `AppState` (or session projection slice) |
| Height | plane compose / shell surface metrics |
| Paint | dedicated todos strip module under `features/` |
| Timeline tool body | `tool_format` presenters (history only) |

TUI **must not**:

- Persist its own todo file.
- Treat latest Timeline tool args as authority after host projection exists.
- Mutate list from UI in v1.

## Plane integration

**Height grant** comes from Dock Stack `solve` (offer → grant). This section
defines the **Todos provider** preferred/min heights and paint under grant.

Stack order is owned by the Dock Stack registry ([dock-coexistence](../features/dock-coexistence.md)):

```text
┌─ plane ─────────────────────────────────────┐
│ STREAM  (grow)                              │
├─────────────────────────────────────────────┤
│ Boundary  1 | Dock Stack-owned, border_muted │
│ Todos?    0 | header + items + overflow? + rule │
│ Suggest?  0 | budget                        │
│ Guidance  1                                 │
│ Composer  editor budget                     │
└─────────────────────────────────────────────┘
```

Paint order in the dock band (top → bottom):

```text
  [Todos header]
  [Todos item …]
  [Todos +N more if any]
  [Dock Stack separator; hosts Suggest title when active]
  [Suggest rows if any]
  [Guidance: notice or active hint]
  [Composer]
```

Suggested height policy (constants, tune in code):

| Condition | Height |
|-----------|--------|
| Feature off / no viewed agent / empty items | `0` |
| Non-empty | `1 (header) + min(items, MAX_ITEM_ROWS) + (1 if overflow)` |
| Cap | e.g. `MAX_ITEM_ROWS = 6` so Stream keeps the frame majority |

Exact numbers are implementation constants; feature PRD requires
**budgeted, non-stream-stealing** and owns the ASCII wireframes.

Compose inputs: current `TodoList` for **viewed** `agent_instance_id` only.

## Client model

```text
// Illustrative

struct TodoListsState {
  by_agent: HashMap<AgentInstanceId, TodoList>,
}

// On snapshot: replace/merge by_agent from todoLists[]
// On TodoListUpdated: by_agent.insert(list.agent_instance_id, list)
// Strip projection: by_agent.get(viewed_agent_id).filter(|l| !l.items.is_empty())
```

Ignore unknown future item fields (serde already drops them on typed
structs). Unknown agents simply absent → empty strip.

## Strip presentation architecture

Prefer a small pure projector + paint:

```text
TodoList + width + max_rows
    → TodoStripView { header, rows, overflow }
    → ratatui Lines in dock rect
```

### Wireframe → rows (normative structure in feature PRD)

Feature doc ASCII is the target silhouette. Mapping:

```text
  Todos  1/3 done · 1 active · 1 remaining     →  header Line
  ✓  Ship protocol serde                       →  item Line (TodoDone)
  ▸  Persist list with session                 →  item Line (TodoActive)
  ·  Dock strip + height budget                →  item Line (TodoPending)
  +2 more                                      →  overflow Line (dim)
```

### Header (one row when strip visible)

```text
  {▾|▸} Todos  {done}/{total} done · {active} active · {remaining} remaining
```

- The expanded/collapsed disclosure mark owns a header-row pointer hit zone.
- Leading label **Todos** (stable product word).
- Counts from the projected list only.
- Do **not** put model/cwd/cost here (BottomBar).
- Truncate trailing counts on very narrow width; keep **Todos** + `done/total`
  if possible.

### Item rows

```text
  {mark}  {content…}
```

| Field | Rule |
|-------|------|
| `mark` | Same family as timeline `todo_*` body (`✓` / `▸` / `·` or shared glyphs) |
| `content` | Single line; truncate with unicode-width ([line-layout](../features/line-layout.md)) |
| id | Not required as a separate column on the strip |
| `detail` | **Omit** on strip in v1 |

### Overflow

```text
  +{n} more
```

When `items.len() > MAX_ITEM_ROWS`: paint first `MAX_ITEM_ROWS` in **list
order**, then one overflow row. Overflow is not a todo item and is not
interactive.

### Status → style (family, not a glyph catalog)

| `TodoStatus` | Presentation intent |
|--------------|---------------------|
| `completed` | Muted; strikethrough on **content text only** (not whole row padding) |
| `in_progress` | Warning/active emphasis |
| `pending` | Dim / secondary |

Reuse the same mapping in Timeline `todo_*` tool body presenters so dock and
history feel consistent.

## Timeline boundary

| Path | Behavior after strip ships |
|------|----------------------------|
| `todo_write` / `todo_read` cards | Normal tool title + optional expand |
| Force-open checklist when collapsed | **Remove** (F-27 / feature acceptance) |
| Expanded body | Typed checklist; may show snapshot of that call’s payload (history), which can **differ** from current dock if the agent wrote again later — that is correct for audit |

Migration: if host lists absent, timeline-derived checklist remains a temporary
fallback only; gate with a clear “projection available” path and delete
fallback when D-39 slice E lands.

## Interaction / hit testing

The whole Todos header row exposes `HitId::TodosToggle`. A primary click
toggles transient `TodoListsState` presentation state directly and recomposes
the Dock Stack on the next frame:

- expanded offer: projected header + items + overflow + separator;
- collapsed offer: header + separator (`TODOS_MIN_HEIGHT`);
- `TodosToggle` hover paints the header text with the shared `accent` token in
  both states; it does not add a row background;
- item and separator rows have no element hit action;
- the strip remains non-focusable and does not steal keys or pointer scrolling.

The collapse flag resets with session-local todo state. It is not included in
host protocol, session snapshots, or settings, and does not mutate todo data.

## Reducers / events

| Input | Effect |
|-------|--------|
| Full session snapshot | Rebuild `by_agent` from projected lists |
| `TodoListUpdated` | Upsert one agent’s list; recompose if viewed |
| Viewed agent change | Rebind strip; height may go 0 |
| Feature flag off (if client-visible) | Force height 0 |
| Session switch | Drop previous session’s maps; load new snapshot |

No local “optimistic” todo writes from the strip.

## Module sketch (implementation)

Prefer a cohesive unit under ~300–400 lines (split if larger):

```text
packages/tui/src/features/todos/
  mod.rs          // public strip API + tests
  state.rs        // optional: projection helpers
  render.rs       // header + rows + overflow (separator remains stack chrome)
```

Wire:

- Snapshot/event apply in app/session reducers.
- Height in `navigation/compose` / plane metrics.
- Paint in main shell render path next to Notice / Suggest.

Do **not** put multi-line checklist logic in BottomBar render.

## Verification

- Empty / non-empty / cap / overflow height math (unit).
- Collapsed offer is exactly header + separator; header hit toggles both ways.
- Viewed-agent switch changes projection (unit).
- Snapshot + live update replace list without Timeline dependency (unit).
- Timeline force-body removed or gated once strip enabled (unit/regression).
- Manual: long session, scroll up, strip still shows current list; switch
  agent; resume session.

## Related

| Doc | Role |
|-----|------|
| [features/todo-list.md](../features/todo-list.md) | TUI behavior contract |
| [features/ui-ux.md](../features/ui-ux.md) | Dock IA + information duties |
| [features/timeline.md](../features/timeline.md) | History tools |
| [design/timeline.md](./timeline.md) | Tool presenters |
| [D-39](../../../../docs/design/D-39-agent-todo-list.md) | Host/orch/protocol slices |
