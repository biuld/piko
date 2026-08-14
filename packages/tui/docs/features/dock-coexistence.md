# Dock Stack (plane coexistence)

> Status: draft  
> Design: [dock-coexistence.md](../design/dock-coexistence.md)  
> Kind: **standalone TUI infrastructure feature** (not a product strip)  
> Consumers: Notice, Todos, Suggest (command palette / `@`), Composer;  
> prerequisite for [todo-list.md](./todo-list.md)

## Overview

**Dock Stack** is a first-class TUI feature that owns **how multiple
non-resident (optional) plane bands appear together** above the composer,
without each feature independently `fixed()`-stacking itself into Stream.

It is **infrastructure + policy**, not a user-facing product surface with its
own slash command or modal. Product bands (Notice, Todos, slash palette, …)
keep their state, paint, and interaction; they **plug into** the dock stack
via a small offer/grant contract.

### Why it exists

| Without Dock Stack | With Dock Stack |
|--------------------|-----------------|
| Each feature sets its own height in `compose_plane` | One solver owns joint budget |
| Notice + palette + future Todos sum unboundedly | Stream keeps a minimum floor |
| New optional band = ad-hoc `if` + layout bugs | Register band + shrink class |
| “Command palette” confused with modals | Suggest is a **dock band**, not `SurfaceIntent::Dock` |

### Scope of this feature

**Owns**

1. **Band catalog** for the plane dock (ids, order, residency class).
2. **Visibility policy hooks** (which bands may show with which others).
3. **Height arbitration** (preferred → granted under `STREAM_MIN` / `DOCK_MAX`).
4. **Composition of plane regions** for dock slots (what `compose_plane` builds
   for optional bands + how Composer/Stream relate).
5. **Extension rules** for adding a new non-resident band without forking
   layout.

**Does not own**

- Notice copy, dismiss, severity ([notice-row](./notice-row.md)).
- Todo list domain or strip row typesetting ([todo-list](./todo-list.md), F-27).
- Slash catalog or accept/submit ([command-surface](./command-surface.md),
  [auto-completion](./auto-completion.md)).
- Editor buffer / caret ([editor](./editor.md)).
- Modal z-stack / `SurfaceIntent` ([base-frame-layout](./base-frame-layout.md)).
- One-row paint helper `dock_line` (shared **widget**, not this stack).

## Core concepts

### Resident vs non-resident

| Class | Role | Examples |
|-------|------|----------|
| **Resident (anchor)** | Always participates in plane layout | **Stream** (grow), **Composer** (min height always) |
| **Non-resident (ephemeral band)** | Height **0** when inactive; may appear without user opening a modal | **Notice**, **Todos**, **Suggest** |

“Non-resident” means **not always occupying chrome**, not “stateless.” Todos
are durable product state but the **strip is non-resident in the layout**
(hide when empty).

### Dock band

A **dock band** is a vertical slice in the plane bottom stack with:

- Stable **BandId**
- Fixed **order** relative to other bands
- **Residency** (ephemeral vs anchor)
- **Shrink class** (how eagerly height is sacrificed)
- Content owned by a **provider feature**

### Offer / grant

```text
Provider feature          Dock Stack                 Layout / paint
     │                        │                            │
     │  DockBandOffer         │                            │
     │  (want visible?,       │                            │
     │   preferred_h,         │                            │
     │   min_h if visible)    │                            │
     ├───────────────────────►│                            │
     │                        │  arbitrate + order         │
     │                        │  DockBandGrant             │
     │                        ├───────────────────────────►│
     │  granted_h + paint rect│                            │
     │◄───────────────────────┤                            │
```

- **Offer** = what the feature wants this frame (pure projection of its state).
- **Grant** = what the stack allows (may be 0, min, or preferred).
- Providers **must** paint within granted height (scroll/truncate inside the
  band); they must **not** assume preferred was honored.

### Not this feature

| Thing | Relation |
|-------|----------|
| `SurfaceIntent::Dock` | Modal ComposerBand (approval / tool workflow). **Different** “dock.” |
| `ui::components::dock_line` | Single-row paint primitive for notice/hints. |
| BottomBar | Shell chrome **outside** plane dock budget. |
| Centered / CoverBody / Select modals | Z-stack; force Suggest offer off while open. |

## Architecture (normative abstraction)

```text
┌─────────────────────────────────────────────────────────────┐
│ AppState / features (NoticeCenter, Todos, AutoComplete, …)  │
│   each builds DockBandOffer for its BandId                  │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│ Dock Stack (this feature)                                   │
│  · Band registry (id, order, shrink class, region mapping)  │
│  · Coexistence rules (matrix / modal gates)                 │
│  · Solver: offers + body_h → grants                         │
│  · Emit plane flex children for granted bands               │
└────────────────────────────┬────────────────────────────────┘
                             ▼
┌─────────────────────────────────────────────────────────────┐
│ navigation compose / layout solve / render by Region        │
└─────────────────────────────────────────────────────────────┘
```

### Band registry (v1 catalog)

| Order (top→bottom) | BandId | Residency | Shrink class | Region | Provider |
|--------------------|--------|-----------|--------------|--------|----------|
| — | `Stream` | anchor grow | never shrink below floor | `Stream` | Timeline |
| 1 | `Notice` | ephemeral | **protect** (0 or 1) | `Notice` | notifications |
| 2 | `Todos` | ephemeral | **durable** (keep header if non-empty) | `Todos` (when added) | todos |
| 3 | `Suggest` | ephemeral | **transient** (shrink first) | `Suggest` | auto_completion |
| 4 | `Composer` | anchor fixed | shrink to editor min last | `Composer` | editor |

Future ephemeral bands register a new `BandId` + order + shrink class; they do
**not** open-code another `if` in compose without going through the registry.

### Provider contract (what every ephemeral band must implement)

Each provider supplies, every frame (or when metrics are built):

| Field | Meaning |
|-------|---------|
| `active` | Whether the band wants to participate (`false` → offer height 0) |
| `preferred_height` | Ideal rows including internal chrome |
| `min_height` | Minimum rows if `active` (e.g. Notice 1; Todos header; Suggest chrome+1+footer) |
| optional `priority_hint` | Only if registry needs tie-break (default = shrink class) |

Dock Stack returns:

| Field | Meaning |
|-------|---------|
| `granted_height` | 0 ⇒ do not paint / no region leaf |
| optional `content_budget` | Rows available inside chrome for lists |

Providers **must not**:

- Call `compose_plane` or allocate flex siblings themselves.
- Expand past `granted_height`.
- Treat Timeline as the layout home for “always visible” optional state that
  belongs in a dock band (product rule for Todos lives in todo-list PRD;
  enforcement of height is here).

### Coexistence policy (v1)

Default: **ephemeral bands are not mutually exclusive** by content.

| Rule | Behavior |
|------|----------|
| Notice ∥ Todos ∥ Suggest ∥ Composer | **Allowed** |
| Slash vs `@` | Same `Suggest` band; providers mutually exclusive inside auto_completion |
| Any product modal open | Suggest `active` forced false (existing gate) |
| `SurfaceIntent::Dock` modal | Suggest off; other plane bands may still grant height |
| Empty / feature-off | Band `active` false → height 0 |

Hard exclusions beyond the table require an explicit registry rule (no
silent one-off `if`s in random features).

### Height arbitration (policy)

Let `body_h` be plane body height (excludes BottomBar).

1. Compute `stream_min` and `dock_max = body_h - stream_min`.
2. Sum preferred heights of Composer + all active ephemeral bands.
3. If sum ≤ `dock_max`, grant preferred.
4. Else shrink by **shrink class order**:
   1. **transient** (Suggest) down toward `min_height`
   2. **durable** (Todos) item rows toward `min_height` (header kept)
   3. **Composer** toward editor minimum
   4. **protect** (Notice) never below min while active
5. Never grant Composer below editor minimum; never grant active Notice 0;
   never grant active Todos 0 if product requires header visibility.

Exact constants live in design; policy order is normative here.

## Layout (ASCII)

### Stack model

```text
┌─ plane (Dock Stack + Stream) ───────────────────────────────┐
│ STREAM                                              grow    │
│                                                             │
│ ── dock stack (solver-owned fixed bands) ─────────────────  │
│ Notice?     ephemeral · protect                             │
│ Todos?      ephemeral · durable                             │
│ Suggest?    ephemeral · transient  (/ palette or @)         │
│ Composer    anchor                                          │
└─────────────────────────────────────────────────────────────┘
  BottomBar   shell chrome — outside Dock Stack
```

### Idle (only anchors)

```text
┌──────────────────────────────────────────────────────────────┐
│ STREAM                                                       │
│  …                                                           │
├──────────────────────────────────────────────────────────────┤
│ › type a message…                                  Composer  │
├──────────────────────────────────────────────────────────────┤
│ agent · model · …                                  BottomBar │
└──────────────────────────────────────────────────────────────┘
```

### Full ephemeral stack — preferred (worst case)

```text
┌──────────────────────────────────────────────────────────────┐
│ STREAM   (at risk without solver)                            │
├──────────────────────────────────────────────────────────────┤
│ ▲  … · F8                                          Notice    │
│ Todos  2/8 done · …                                Todos     │
│ ✓  …                                                         │
│ ▸  …                                                         │
│ ·  … × N                                                     │
│ +k more                                                      │
│ ─ slash commands ─────────────── [i/n] ─           Suggest   │
│ ❯ /model …                                                   │
│   … (command palette = this band, not a modal)               │
│ Tab · Enter                                                  │
│ /mo                                                Composer  │
└──────────────────────────────────────────────────────────────┘
```

### Full stack — after solver grant (illustrative)

```text
┌──────────────────────────────────────────────────────────────┐
│ STREAM   (≥ stream_min)                                      │
│  … still readable …                                          │
├──────────────────────────────────────────────────────────────┤
│ ▲  … · F8                                          Notice    │
│ Todos  2/8 done · …                                Todos     │
│ ▸  active item only…                                         │
│ +6 more                                                      │
│ ─ slash commands ─ [i/n] ─  (fewer rows)           Suggest   │
│ ❯ /model …                                                   │
│ /mo                                                Composer  │
└──────────────────────────────────────────────────────────────┘
```

## Behavior / interactions

Dock Stack itself has **no keybindings** and **no focus owner**.

| Concern | Owner after grant |
|---------|-------------------|
| Esc closes palette | Suggest / editor (existing Esc priority) |
| F8 dismiss notice | Notice |
| Type in composer | Composer |
| Todo strip clicks | Todos feature (v1 none) |

Focus and hit-testing still use `Region::*` rects produced from **grants**.

## Configuration

| Item | v1 |
|------|-----|
| User-facing settings | None required |
| Tunables | Implementation constants (`STREAM_MIN_*`, per-band caps) in dock stack module |
| Keybindings | None |

## Non-goals

- Owning product data for any band.
- Replacing modal layout (`SurfaceIntent`, ComposerBand budgets).
- A unified “dock widget” that paints all bands (bands keep their renderers).
- Auto-dismissing Suggest when height is tight (v1 shrink only).
- Session-global layout preferences UI (later).

## Acceptance

- [ ] Dock Stack is a **named module** with registry + solver + tests (not only
      comments in `compose.rs`).
- [ ] Notice, Suggest, Composer go through offer → grant (Todos when added).
- [ ] Adding a hypothetical band is documented as: register + provider offer;
      no third `if` pile-up without registry.
- [ ] Coexistence: Notice+Suggest+Composer simultaneous; Stream ≥ floor on
      short `body_h`.
- [ ] Shrink order unit-tested (transient before durable before composer min).
- [ ] Modal open ⇒ Suggest grant 0.
- [ ] Docs distinguish plane Dock Stack vs `SurfaceIntent::Dock` vs `dock_line`.
- [ ] [todo-list](./todo-list.md) depends on this feature for height, not a
      private fixed stack.

## Related

| Doc | Role |
|-----|------|
| [design/dock-coexistence.md](../design/dock-coexistence.md) | Module layout, types, algorithm, migration |
| [todo-list.md](./todo-list.md) | Ephemeral durable band consumer |
| [auto-completion.md](./auto-completion.md) | Transient Suggest / palette provider |
| [notice-row.md](./notice-row.md) | Protect Notice provider |
| [editor.md](./editor.md) | Composer anchor |
| [ui-ux.md](./ui-ux.md) | Shell IA parent |
| [base-frame-layout.md](./base-frame-layout.md) | Frame recipe |
| [single-line-dock.md](./single-line-dock.md) | `dock_line` paint helper only |
