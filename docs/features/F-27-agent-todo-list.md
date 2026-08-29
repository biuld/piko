# F-27: Agent todo list

> Status: draft
> Priority: P1
> Source evidence: piko product direction (long-horizon goal compression);
> orchd `todo` tool family (baseline write path only)

## Amendment (2026-08-29): explicit Todo overlay

The TUI no longer keeps Todo in the Dock Stack. The viewed agent's current
list is opened explicitly with `/todo` in a centered, read-only overlay. The
Dock Stack remains visually quiet and does not change height when todo state
changes. This amendment supersedes earlier dock-strip language in this PRD.

## Amendment (2026-08-10): missing-status default

Models occasionally emit a `todo_write` item without `status`. Rejecting the
entire list on that omission voids the agent's plan for a one-field mistake.
Rev B defaults a **missing** `status` to `pending` while still rejecting
**unknown** `status` values. Rejection errors become actionable: they name
the failing item index, the offending field, and the accepted values.

## Terminology (canonical)

| Layer | Term | Notes |
|-------|------|--------|
| **Product / docs / UI** | **todo list** (prose); overlay may say **Todos** | Long-horizon goal compression for one agent. One concept. |
| **One item** | **todo** / **todo item** | Not “task” / “task item” (avoids F-01 background task and multi-agent “task” wording). |
| **Domain / protocol (new)** | `TodoList`, `TodoItem`, `TodoListUpdated` | Host projection and snapshot types. |
| **Tool family** | feature `todo`; tools `todo_write` / `todo_read`; args `todos` | Same concept as the product todo list; model-facing ids stay as-is. |
| **Avoid** | product name **task list** / types `TaskList` / `TaskItem` | Collides with F-01 `Task*` and multi-agent “follow-up task” language. |
| **Avoid** | session-global “the todo” without agent scope | Lists are **per AgentInstance**. |

**Rule:** product behavior and types use **todo list** / `TodoList`. Tools are
the write/read API for that same list—not a second domain.

## Summary

Each **AgentInstance** owns a structured **todo list**: a lossy, agent-curated
compression of long-horizon goals and progress. Agents update it through
`todo_write` / `todo_read`; users always see the **current** list for the
viewed agent through an explicit **`/todo` overlay** (not only as historical
tool cards, and not in BottomBar chrome). The list is host-authoritative user-visible
state so it survives reconnect and can grow richer item fields without
redesigning the shell.

## Problem

1. **Drift on long work.** Multi-step coding sessions lose the plan in
   transcript noise. A compact **todo list** is the primary anti-drift surface
   for both the model and the human.
2. **State is not client-visible as state.** Today `todo_write` / `todo_read`
   keep list data in orchd keyed by agent, but clients only see **tool
   transcript events**. There is no projected “current todo list” for the
   viewed agent, so the UI cannot present live state independent of history.
3. **Wrong surface if forced into chrome.** BottomBar is a single-line session
   meter (model, cwd, context, cost). A multi-item todo list does not belong
   there. Forcing the latest `todo_write` card open in Timeline still **scrolls
   away** and confuses history with live product state.
4. **Extension will outgrow tool-arg JSON in the UI.** Planned per-item fields
   (notes, links, acceptance criteria, ownership, …) need a stable **TodoList**
   domain and projection shape, not ad-hoc parsing of every tool call in the TUI.

## User journeys

1. User starts long-horizon work. The agent writes an initial todo list. A
   user opens `/todo`; the overlay shows current items for the **viewed**
   agent (progress + active item at a glance). Timeline still records the tool
   call as history (collapsed by default).
2. The agent marks items completed / in progress via tools. The open overlay
   updates **in place** without the user scrolling to the latest tool card.
3. User switches viewed agent (F4 / `/agents`). `/todo` shows **that**
   agent’s list (or empty). Parent and child lists never mix.
4. User reconnects or resumes the session. Snapshot restores each agent’s
   current list; the overlay matches pre-disconnect state for the viewed agent.
5. (Later) Items carry richer metadata; the strip stays a scannable summary
   while expanded detail (dock expand or dedicated surface) can show more.

## In scope

- **Agent-scoped** todo list as first-class product state (one current list per
  AgentInstance).
- Normative **serde** for `TodoStatus` / `TodoItem` / `TodoList` (see below).
- **Durable persistence** with the session (resume-safe; survives compaction).
- Tool surface to replace/read the list (`todo_write` / `todo_read`; keep
  replace-list semantics unless product later adds patch).
- **Host-authoritative projection** of current lists into session snapshot and
  live events so any client can render without replaying tools.
- **Core prompt injection:** non-empty list rendered into the frozen run prompt
  plus drive instruction to complete remaining items (anti-drift).
- **TUI `/todo` overlay** for the viewed agent's current list.
- Timeline: tool cards remain audit history; they are **not** the live todo
  surface (no requirement to force-expand checklist bodies once the strip
  exists).
- Feature flag alignment with managed feature `todo` (F-18).

## Out of scope (this feature’s first cut)

- BottomBar full list or multi-line checklist in chrome.
- Session-global single list shared by all agents (explicitly **agent-level**).
- User-editable overlay (drag reorder, click-complete) unless a later PRD adds
  host commands; v1 is **read-only projection** of agent-maintained state.
- Cross-session boards, external issue trackers, or human-only lists
  disconnected from the agent.
- Replacing plan/approval workflows (F-07) or multi-agent supervision tools
  (F-10/F-21).
- Full design of every future item extension field (only extensibility rules).

## Behavior and states

### Authority and scope

| Concern | Rule |
|---------|------|
| Owner | AgentInstance (same identity users switch in the agent surface) |
| Writer | Agent tools (and any future host command that intentionally mutates) |
| Reader | Agents via tools; clients via host projection |
| Empty | `/todo` opens with an explicit empty message |
| Non-empty | Dock strip visible for the **viewed** agent |

### Item model and serde (normative)

**Product intent:** the list is a **lossy compression of goals**, not a full
transcript. Agents should update it when plan or progress changes so long runs
stay aligned.

#### Wire conventions

- Protocol structs use **`camelCase`** field names (same as session / agent
  DTOs).
- Status enum variants serialize as **`snake_case`** strings:
  `pending`, `in_progress`, `completed` (matches current tool args).
- **Do not** set `deny_unknown_fields` on items: unknown keys are ignored on
  deserialize so clients can forward-compat additive fields.
- Optional fields use `skip_serializing_if = "Option::is_none"` (and empty
  maps omitted when present).

#### `TodoStatus`

```text
// serde: rename_all = "snake_case"
TodoStatus = pending | in_progress | completed
```

#### `TodoItem`

```text
// serde: rename_all = "camelCase"
TodoItem {
  id: string
  status: TodoStatus
  content: string
  detail?: string          // optional longer note; v1 may be unused
  // future additive optional fields (acceptance, links, …) without breaking v1
}
```

| Field | Required | Rules |
|-------|----------|--------|
| `id` | yes | **String** on the wire. Tool/model may still send JSON numbers; adapters **normalize to decimal string** (`1` → `"1"`). Stable within one agent’s list across rewrites when the agent keeps the same id. |
| `status` | no (defaults to `pending`) | One of the three enum values above. **Missing status normalizes to `pending`**; unknown status → reject the write with an item-indexed error (design: reject invalid values only). |
| `content` | yes | Non-empty after trim for new/updated items; empty content rejected. |
| `detail` | no | Free text; omitted when null/absent. |

#### `TodoList` (durable + projected)

```text
// serde: rename_all = "camelCase"
TodoList {
  agentInstanceId: AgentInstanceId   // string form on wire
  items: TodoItem[]
  updatedAt: i64                     // epoch ms, host or orch clock
  revision: u64                      // monotonic per agent list; starts at 0
}
```

| Field | Rules |
|-------|--------|
| `agentInstanceId` | Owner agent; list is never shared across instances. |
| `items` | Ordered; full replace on `todo_write`. Empty array = cleared list. |
| `updatedAt` | Set on every successful mutation. |
| `revision` | Increment on every successful mutation; clients may ignore. |

Snapshot may carry `todoLists: TodoList[]` or a map keyed by agent instance
id. Live event: `TodoListUpdated` with the full `TodoList` (or equivalent).

#### Tool JSON (`todo_write` / `todo_read`) vs protocol

Tools keep the familiar **`todos`** array key for the model:

```json
// todo_write arguments
{ "todos": [ { "id": 1, "status": "in_progress", "content": "…" } ] }

// todo_read / preferred write result
{ "todos": [ { "id": "1", "status": "in_progress", "content": "…" } ] }
```

`status` may be omitted on any item; normalization defaults it to `pending`,
so a model that emits a partial item cannot silently void the plan. Unknown
`status` values still reject the write with an actionable, item-indexed
error.

| Tool JSON | Protocol |
|-----------|----------|
| top-level `todos` | `TodoList.items` |
| item `id` number or string | always string after normalize |
| item `status` / `content` | same |
| (no agent id in tool args) | `agentInstanceId` from execution context |

Orch/host normalize tool payloads into `TodoItem` / `TodoList` before persist
and projection. `todo_write` **should** return the normalized list (not only
`{ "updated": true }`) so simple clients can refresh without a separate read.

### Persistence (normative)

- The current `TodoList` per agent is **durable session state** (F-09), not
  transcript-only and not orch process memory alone.
- Stored with the session so **resume / reconnect / host restart** restores
  every agent’s list.
- **Compaction must not drop** todo lists (they are not rollup-able chat).
- Managed feature `todo` off: no tools; durable lists may remain on disk but
  are not projected or injected until re-enabled (design may still keep bytes).

### Prompt injection (normative — anti-drift)

When feature `todo` is enabled, every frozen run prompt for an agent **must**
make the model aware of that agent’s current todo list:

1. **List fragment (data):** if `items` is non-empty, inject a dedicated
   prompt fragment (not mixed into free-form tool history) rendering the
   full current list with status and content (and remaining count). Empty
   list → omit the data fragment (no noise).
2. **Drive instruction:** standing instruction (system / agent policy block
   when tools are available) that the model should:
   - treat the todo list as the **lossy plan** for long-horizon work;
   - **prefer finishing remaining** `pending` / `in_progress` items unless
     the user redirects;
   - call `todo_write` when the plan or progress changes so the list stays
     true.
3. **Source of truth for the fragment** is the **durable projected list**,
   not “latest tool card in transcript.”
4. Fragment is **per agent instance** (child runs see their own list only).
5. Not the same as F-03/F-04 world-state run identity (`session_id`, model,
   …): todo list is a **separate fragment class** (e.g. `todo.list`) with
   its own source identity; cache scope follows F-03 rules for non-stable
   per-run context.

### Tool behavior

- `todo_write`: replaces the agent’s **entire** current list; persists;
  projects; returns normalized `todos` (should). Rejection errors are
  actionable: they name the failing item index, the offending field, and the
  accepted values, so the model can correct and retry in the same turn.
- `todo_read`: returns the current list for the calling agent.
- Disabled feature `todo`: tools absent / fail closed per F-18; no list
  fragment; `/todo` shows an empty state.

### Client projection

- Session snapshot includes each agent’s current `TodoList`.
- Live updates emit when a list changes so clients do not wait for full
  snapshot.
- Viewed-agent switch rebinds the overlay without refetching the whole session
  if snapshot already holds all agents’ lists.

### TUI Todo overlay (product)

```text
STREAM     conversation (todo tool cards = history)
DOCK       blank reserved row · Suggest? · Guidance · Composer
MODAL      /todo (viewed agent, explicit)
CHROME     BottomBar (unchanged duties)
```

- **When:** user invokes `/todo`; it also opens for empty state.
- **What:** scannable todo summary (counts + items or compact active focus);
  exact row layout is presentation, not this PRD’s glyph catalog.
- **Not:** BottomBar item; not a Dock Stack band.
- **Interaction (v1):** read-only, dismissible, and internally scrollable.

**TUI presentation contract** (placement, overlay vs Timeline, viewport,
status language family, acceptance): package docs
[todo-list feature](../../packages/tui/docs/features/todo-list.md) and
[todo-list design](../../packages/tui/docs/design/todo-list.md).
This feature owns product/serde/persist/prompt; the TUI docs own client surfaces.

### Timeline

- `todo_write` / `todo_read` remain normal tool projections (audit).
- Live truth is the Todo overlay + host projection, not “last expanded card.”

### Lifecycle

| Event | List behavior |
|-------|----------------|
| Agent created | Empty |
| `todo_write` | Replace current list |
| Agent closed / session end | Durable with session per F-09 expectations for agent-scoped state (design: store with agent or session payload) |
| Resume / reconnect | Restore from host snapshot |
| Compaction | Must **not** drop current list; list is not transcript-only |
| Run prompt | Non-empty list → data fragment; drive instruction when tools available |

## Acceptance criteria

- [ ] Current todo list is **agent-scoped** and readable without scanning
      Timeline for the latest `todo_write`.
- [ ] Host snapshot + live path expose the list; TUI dock shows the viewed
      agent’s non-empty list while scrolled away from tool history.
- [ ] Switching viewed agent switches the strip contents.
- [ ] Resume/reconnect restores lists for agents in the session.
- [ ] Compaction leaves durable todo lists intact.
- [ ] `TodoItem` / `TodoList` serde matches this PRD (camelCase fields,
      snake_case status, string ids after normalize).
- [ ] Each agent run with feature on and non-empty list injects the list into
      the frozen prompt; drive text steers toward completing remaining items.
- [ ] BottomBar duties unchanged (no full checklist there).
- [ ] Managed feature `todo` off → no tools, no strip, no list fragment.
- [ ] Timeline tool cards no longer required to force-show checklist bodies
      once the strip ships.
- [ ] Item schema allows additive fields without breaking v1 projection.
- [ ] `todo_write` accepts items with a missing `status` and normalizes them
      to `pending`; unknown `status` values reject the write with an
      item-indexed, actionable error (Rev B).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Scope | **AgentInstance**, not session-global | Multi-agent work needs separate plans; matches orchd keying and viewed-agent UX |
| Live UI home | **Dock strip**, not BottomBar, not Timeline-only | Always in view; chrome stays meters; history stays history |
| Authority | **Host-projected** + **durable** product state | hostd authority; resume and anti-drift require disk, not orch RAM |
| Edit surface v1 | Agent tools write; **client read-only** | Avoid dual-writer races until explicit human-edit commands exist |
| Semantic role | **Todo list** = goal compression for long-horizon | Core anti-drift artifact; extensible per-item metadata later |
| Product term | **todo list** / types `TodoList` | Aligns with tools; avoids F-01/multi-agent “task” collision |
| Item id wire type | **string** (normalize numbers from tools) | Stable, extensible; tool JSON stays model-friendly |
| Replace vs patch | **Full replace** on write (v1) | Simple, matches current tool; patch can be a later tool |
| Missing `status` | **Default to `pending`** | Model output is lossy; a one-field omission should not void the whole plan. Unknown values still fail closed. |
| Empty UI | **Hide strip** | Density; empty is not a status chrome fact |
| Prompt | **Always inject non-empty list** + drive to finish remaining | Model must see plan without tool_read; prevents long-run drift |
| World-state | **Separate** todo fragment, not F-03 run-identity world-state | Different lifecycle and cache/update rules |

## Fusion decisions (codex-rs)

Not derived from a codex-rs feature PRD. Any external agent “plan” or todo
tool is **modeling reference only**; piko owns agent-scoped host projection and
dock placement.

| External pattern | Decision | piko landing / rationale |
|---|---|---|
| Tool-only plan in transcript | **kept (adapted)** | Tools remain; live UI uses projected state |
| Editor-local todo UI without host | **rejected** | hostd authority |
| Session-global single plan | **rejected** (for v1) | multi-agent agent-level lists |

## Open questions

1. **Persistence file layout:** field on agent instance record vs map on
   `session.json` (D-39 chooses a default; may refine).
2. **Overlay density:** long lists scroll within the centered viewport (TUI
   feature/design docs).
3. **Parent visibility:** should a parent’s overlay optionally summarize children
   lists? Deferred; default is viewed agent only.
4. **Human editing:** `/todo` is read-only; editing requires a later host-backed
   command contract.
5. **Exact drive-instruction copy** and block placement relative to agent
   base instructions (D-39 template; product may tune wording).

## Reference evidence

- orchd `TodoProvider` (per-`agent_id` list, `todo_write` / `todo_read`)
- F-18 managed feature `todo`
- F-09 session persistence; F-10/F-21 multi-agent; F-22 client projection
- TUI plane: Notice / Suggest / Composer dock stack; BottomBar chrome contract
- Product discussion: dock option B; todo list as long-horizon goal compression
- Terminology: prefer todo over task to avoid F-01 TaskRegistry / multi-agent
  “task” overload
