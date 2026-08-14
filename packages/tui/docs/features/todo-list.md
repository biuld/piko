# Agent Todo List (TUI)

> Status: draft  
> Parent product: [F-27](../../../../docs/features/F-27-agent-todo-list.md)  
> Design: [todo-list.md](../design/todo-list.md)  
> **Prerequisite:** [dock-coexistence.md](./dock-coexistence.md) — **Dock Stack**
> infrastructure (band offer/grant); this strip is a **provider**, not a private
> `compose_plane` branch

## Overview

The TUI presents each **viewed agent’s current todo list** as **dock-resident
product state**, not as “whatever the last `todo_write` tool card showed.”

**Terminology** (same as F-27): product/docs/UI = **todo list**; chrome may
label **Todos**; protocol = `TodoList` / `TodoItem`; tools remain `todo_*`.
Do not call this surface a **task list**.

Goals:

1. **Always visible when non-empty** while the user is in the chat shell
   (scroll-independent of Timeline).
2. **Scannable progress** — counts + remaining work without opening tools.
3. **Projection-only** — host snapshot / live events only; no client-invented
   lists and no dual-writer edits in v1.
4. **Clear split with Timeline** — tool cards are **audit history**; the dock
   strip is **live truth**.

## Layout (ASCII)

Glyphs below are **illustrative** of the checklist family already used in
timeline todo presenters (`✓` done, `▸` active, `·` pending). Exact codepoints
and theme tokens stay in code; structure and information duties are normative.

### Shell — list non-empty (live strip)

```text
┌──────────────────────────────────────────────────────────────┐
│ STREAM (scrolls independently)                               │
│                                                              │
│  … older messages …                                          │
│  ▸ todo_write  1/3 done                          ✓  ~…       │  ← history
│  $ cargo test …                                  exit 0      │     only
│  … user scrolled up; tool cards off-screen …                 │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ DOCK                                                         │
│  ▲  something optional notice · F8                           │  Notice?
│  Todos  1/3 done · 1 active · 1 remaining                    │  ┐
│  ✓  Ship protocol serde                                      │  │ Todos
│  ▸  Persist list with session                                │  │ strip
│  ·  Dock strip + height budget                               │  ┘
│  /resume  /rename  …                                         │  Suggest?
│  ──────────────────────────────────────────────────────────  │
│  › type a message…                                           │  Composer
├──────────────────────────────────────────────────────────────┤
│ agent · model · ~/proj · 12k/200k · $0.12                    │  BottomBar
└──────────────────────────────────────────────────────────────┘
```

### Shell — list empty (or feature off)

```text
┌──────────────────────────────────────────────────────────────┐
│ STREAM                                                       │
│  … conversation …                                            │
├──────────────────────────────────────────────────────────────┤
│ DOCK                                                         │
│  (no Todos rows — height 0)                                  │
│  › type a message…                                           │
├──────────────────────────────────────────────────────────────┤
│ agent · model · ~/proj · …                                   │
└──────────────────────────────────────────────────────────────┘
```

### Dock strip anatomy

```text
  Todos  1/3 done · 1 active · 1 remaining     ← header (1 row)
  ✓  Ship protocol serde                       ← completed (content may strike)
  ▸  Persist list with session                 ← in_progress (emphasis)
  ·  Dock strip + height budget                ← pending (muted)
  ^  ^
  |  content (truncate to width; no id prefix required on strip)
  status mark (family shared with timeline)
```

Header **must** carry: product label (**Todos**) + progress counts.  
Each item row **must** carry: status + `content`.  
v1: **no** `detail` second line on the strip; **no** serial numbers unless
content itself includes them.

### Overflow (height cap)

When `items` exceed the budget (header + ≤ N item rows):

```text
  Todos  2/8 done · 1 active · 5 remaining
  ✓  First completed step
  ▸  Currently doing this
  ·  Next pending A
  ·  Next pending B
  ·  Next pending C
  +3 more                                          ← overflow (not a real item)
```

- Visible items stay in **list order** (full-replace semantics).
- Overflow is one dim summary row; not a focusable fake todo.

### Timeline vs dock (same session)

```text
STREAM (audit — may be scrolled away)
  ▸ todo_write  1/3 done                    ✓  ~…
      progress  ✓1  ~1  ·1
      ✓  Ship protocol serde
      ▸  Persist list with session
      ·  Dock strip + height budget
  … later turns …
  ▸ read  src/foo.rs                        ✓  ~…

DOCK (live — always if non-empty for viewed agent)
  Todos  2/3 done · 0 active · 1 remaining
  ✓  Ship protocol serde
  ✓  Persist list with session             ← newer write; strip updated
  ·  Dock strip + height budget            ← Timeline old card can lag
```

| Surface | Truth |
|---------|--------|
| Dock | **Current** host-projected `TodoList` |
| Timeline card body | **That call’s** payload (history); may differ after later writes |

Collapsed tool card after strip ships (no force-open body):

```text
  ▸ todo_write  2/3 done                    ✓  ~…
```

### Viewed agent switch

```text
  [view agent A]                         [view agent B]
  Todos  1/2 done                        (strip height 0 if B empty)
  ✓  A's work
  ▸  A's next
```

Parent and child lists never merge in one strip.

### Placement rules

| Rule | Value |
|------|--------|
| Plane slot | **Dock**, after Notice, **before** Suggest |
| Visibility | Height **0** when empty, feature off, or no list for viewed agent |
| Scope | **Viewed AgentInstance only** |
| Not in | BottomBar, Editor body, permanent Stream pin |
| Dock Stack | Implements [DockBandOffer](./dock-coexistence.md) for `BandId::Todos`; paints only within **grant**; never allocates its own flex sibling |

## Surfaces

### A. Dock strip (live)

Primary user-facing surface for the **current** list. Wireframe above is the
normative **structure**; paint details stay in code.

**Must show** (non-empty, feature on, projection present)

- Header: **Todos** + progress counts.
- Ordered item rows: status mark + content (width-truncated).
- Enough rows (within budget) to see done / active / pending at a glance.

**Should show**

- Overflow `+N more` when capped.
- Shared checklist family with expanded timeline `todo_*` bodies.

**Must not show**

- Another agent’s items while that agent is not viewed.
- Items not in the host projection.
- Full multi-line checklist in BottomBar.
- Invented complete lists when projection is missing.

**Height**

- Budgeted in plane compose. Never steals the whole stream.
- Cap = header + ≤ N items + optional overflow row (N is an implementation
  constant; design doc names it).

**Interaction (v1)**

| Gesture | Behavior |
|---------|----------|
| Keyboard / mouse on strip | **Read-only** — no complete/reorder/edit |
| Focus | Strip is **not** a focus owner in v1 (no Tab stop) |
| Expand | Optional later; v1 cap + overflow is enough |
| Click item | No host mutation |

### B. Timeline tool cards (history)

`todo_write` / `todo_read` remain normal **tool projections** in the stream
(see ASCII “Timeline vs dock”).

| Concern | Rule |
|---------|------|
| Role | Audit: “the agent wrote/read this list at that turn” |
| Live truth | **Dock** + host projection |
| Default density | Collapsed title OK once strip ships |
| Body when expanded | Same checklist family; not raw wire JSON |

Migration without host projection may still force a useful body from tool
args; that is temporary fallback only.

### C. What is not a todo surface

| Surface | Why not |
|---------|---------|
| BottomBar | Single-line session meters only |
| Notice row | Ephemeral attention, not durable plan state |
| Composer | Draft input; must not host the list body |
| Agent Select panel | Switch agent; optional later count chip only |

## Data binding

| Source | Use |
|--------|-----|
| Session snapshot `todoLists` (or equivalent) | Seed per-agent maps on apply |
| Live `TodoListUpdated` (or equivalent) | Replace that agent’s list in place |
| Viewed agent id | Selects which list paints the strip |
| Managed feature `todo` off | No strip; ignore projection for display |

Empty `items` → hide strip (height 0).  
Missing projection for an agent → treat as empty (do not invent from Timeline
once host path exists).

## State language (family)

Todo item status maps to the **shared** success / warning / muted language
([component-feedback](./component-feedback.md), [ui-ux](./ui-ux.md)):

| Status | Role in UI |
|--------|------------|
| `completed` | Done — de-emphasized; content may use strikethrough on the **text** only |
| `in_progress` | Active — emphasis (e.g. warning/accent family) |
| `pending` | Remaining — muted |

Exact glyphs and theme tokens are **presentation code**, not this PRD’s
catalog. The strip and timeline presenters should feel like **one family**.

## Configuration

| Item | v1 |
|------|-----|
| User settings for strip on/off | None required; visibility follows non-empty + feature |
| Keybinding | None required for read-only strip |
| Height cap | Implementation constant in compose (document in design) |

## Non-goals

- Per-projection layout blueprints for every future `detail` field (code).
- Client-only todo truth derived forever from replaying Timeline.
- Session-global single list UI.
- BottomBar multi-line checklist.
- Human-editable strip without host commands.
- OS notifications for todo changes.
- Documenting every glyph/codepoint used in checklist rows.

## Acceptance (TUI)

- [ ] Non-empty viewed-agent list appears in dock above composer while user has
      scrolled Timeline away from the last `todo_write`.
- [ ] Switching viewed agent switches strip contents (or hides if empty).
- [ ] Empty list removes strip height (no permanent empty chrome).
- [ ] Feature `todo` off → no strip.
- [ ] Timeline todo tools do not need force-expanded bodies once strip is live.
- [ ] Strip never shows another agent’s items for the current view.
- [ ] No mutations from strip clicks/keys in v1.

## Related

| Doc | Role |
|-----|------|
| [F-27](../../../../docs/features/F-27-agent-todo-list.md) | Product + serde + persist + prompt |
| [D-39](../../../../docs/design/D-39-agent-todo-list.md) | Cross-crate projection design |
| [ui-ux.md](./ui-ux.md) | Shell IA + Todos information duties |
| [timeline.md](./timeline.md) | Stream history behavior |
| [line-layout.md](./line-layout.md) | Shared column math if strip rows need left/right chrome |
| [dock-coexistence.md](./dock-coexistence.md) | **Prerequisite** — stack + height arbitration with palette |
| [design/todo-list.md](../design/todo-list.md) | TUI modules, compose budget, reducers |
