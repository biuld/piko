# Dock Stack Design

> Status: draft  
> Feature: [dock-coexistence.md](../features/dock-coexistence.md)  
> Kind: **TUI infrastructure** — plane ephemeral-band coordinator

## Goal

Implement **Dock Stack** as a standalone layout subsystem that:

1. Registers plane dock bands (resident + non-resident).
2. Collects per-frame **offers** from provider features.
3. **Solves** joint height under Stream floor / dock max.
4. Emits **grants** and the flex children for `compose_plane`.

Product features never allocate their own plane siblings outside this path.

## Placement in crate architecture

```text
piko-tui-layout     flex / modal geometry (no product BandId)
        ▲
navigation/         Region, SurfaceId, compose_plane entry
        ▲
features/dock_stack/   ← THIS FEATURE (solver + registry + types)
        ▲
features/{notifications, todos, auto_completion, editor}
        offer builders + paint within grant
layout/mod.rs       plane_metrics: gather offers → solve → metrics/grants
render/             match Region → feature paint(granted rect)
```

| Layer | Responsibility |
|-------|----------------|
| `piko-tui-layout` | Unaware of Todos/Suggest/Guidance |
| `features/dock_stack` | Band registry, `DockBandOffer` / `DockBandGrant`, `solve`, shrink policy |
| `navigation/compose.rs` | Thin: `compose_plane(grants)` → `Node<Region>` |
| Provider features | Domain state → offer; grant → paint |
| `layout/mod.rs` | Frame glue: read app → offers → solve → store grants if needed for paint |

**Do not** bury the solver as private helpers only inside `compose.rs` without
a module boundary; the feature PRD requires a **named** dock stack unit.

## Module sketch

```text
packages/tui/src/features/dock_stack/
  mod.rs           // pub API: solve, types, re-exports
  band.rs          // BandId, BandSpec, ShrinkClass, Residency
  offer.rs         // DockBandOffer, DockOfferSet
  grant.rs         // DockBandGrant, DockGrantSet
  solve.rs         // solve(offers, body_h) -> grants
  registry.rs      // static v1 catalog + order
  tests.rs
```

Optional later: `navigation/dock_stack` if you prefer navigation ownership —
**prefer `features/dock_stack`** so it sits with other product-facing features
while remaining infrastructure. `navigation` only maps grants → regions.

File size: keep each file under the project ceiling; solver + tests may split.

## Types (illustrative)

```rust
/// Stable band identity in the plane dock stack.
pub enum BandId {
    Todos,
    Suggest,
    Guidance,
    Composer,
    // Stream is not a "band offer" — anchor grow handled beside solve
}

pub enum Residency {
    /// Height 0 when inactive.
    Ephemeral,
    /// Always participates (Guidance / Composer).
    Anchor,
}

/// How the solver sacrifices height under pressure (feature PRD order).
pub enum ShrinkClass {
    /// Suggest / command palette / @ browser
    Transient,
    /// Todos strip — keep min header while active
    Durable,
    /// Guidance — preserve its resident row
    Protect,
    /// Composer — shrink toward editor min after ephemeral classes
    Anchor,
}

pub struct BandSpec {
    pub id: BandId,
    /// Top-to-bottom order among dock bands (not including Stream).
    pub order: u8,
    pub residency: Residency,
    pub shrink: ShrinkClass,
    pub region: Region, // or map BandId → Region at compose time
}

pub struct DockBandOffer {
    pub id: BandId,
    /// False → treat preferred/min as 0.
    pub active: bool,
    pub preferred_height: u16,
    /// Honored while active if dock_max allows (after higher-yield shrinks).
    pub min_height: u16,
}

pub struct DockBandGrant {
    pub id: BandId,
    pub height: u16, // 0 = omit region leaf
}

pub struct DockSolveInput {
    pub body_height: u16,
    pub offers: Vec<DockBandOffer>, // or fixed struct of four
}

pub struct DockSolveOutput {
    pub grants: Vec<DockBandGrant>,
    pub stream_min: u16,
    pub dock_max: u16,
}
```

Registry returns `&'static [BandSpec]` for v1. Unknown future bands require
registry update (explicit, reviewable).

### Offer set helpers

```rust
// layout/plane_metrics or dock_stack::collect
fn collect_offers(app: &AppState, body: Rect) -> Vec<DockBandOffer> {
  vec![
    todos_offer(app, body.width),   // 0 active until feature lands
    suggest_offer(app),             // active false if modal
    guidance_offer(),               // resident, preferred=min=1
    composer_offer(app, body.width),
  ]
}
```

Each `*_offer` lives next to its feature (or thin adapters in `dock_stack`
that call into feature modules) so domain rules stay with owners.

## Solver

```text
solve(input):
  specs = registry()
  stream_min = max(STREAM_MIN_ABS, ratio(body_h))
  dock_max = body_h.saturating_sub(stream_min)

  // Normalize offers: inactive → height 0; clamp preferred >= min when active
  // Align offers to specs.order

  grants = preferred heights
  sum = Σ grants

  shrink_loop(Transient):  while sum > dock_max && can_shrink(Suggest)
  shrink_loop(Durable):    while sum > dock_max && can_shrink(Todos)
  shrink_loop(Anchor):     while sum > dock_max && composer > COMPOSER_MIN
  // Protect: keep resident Guidance at one row in healthy frames

  return grants + diagnostics (stream_min, dock_max)
```

`can_shrink(band)` = `height > min_height` while active.

**v1:** never set Suggest grant to 0 solely due to budget if `active` (user is
mid-command); only reduce toward `min_height`. Same for Todos header.

Pure functions — unit test without AppState.

## Composition

```rust
pub fn compose_plane(grants: &DockSolveOutput, /* stream always */) -> Node<Region> {
    let mut children = vec![FlexItem::grow(1, leaf(Region::Stream))];
    for g in grants.bands_in_order() {
        if g.height > 0 {
            children.push(FlexItem::fixed(g.height, leaf(g.region())));
        }
    }
    flex_column(children)
}
```

Composer grant is always &gt; 0 in healthy frames.

### Region

Add `Region::Todos` when the todos band ships. Registry maps `BandId::Todos`
→ `Region::Todos`. Until then, `todos_offer.active = false` always.

## Provider integration patterns

### Guidance

```text
active = true
preferred = min = 1
paint: notice or active interaction hint in Region::Guidance
```

### Suggest (command palette / @)

```text
active = has_visible_suggestions && modal.is_none()
preferred = suggestion_height(count)  // existing formula as preferred only
min = chrome + 1 content while active
paint: pass grant.height into pane layout so list rows = f(grant)
```

### Todos

```text
active = feature on && viewed list non-empty
preferred = 1 + min(items, TODOS_MAX_ITEMS) + overflow?
min = 1 (header only) or 1+1 (header + one item) — product pick in todo-list
paint: TodoStripView { max_items derived from grant }
```

### Composer

```text
active = true
preferred = editor.visible_height(...)
min = editor minimum rows
paint: unchanged, rect height = grant
```

## Anti-patterns (reject in review)

| Anti-pattern | Why |
|--------------|-----|
| Feature calls `FlexItem::fixed` for a new plane band outside solver | Breaks budget |
| Hard-code Guidance/Suggest/Todos heights only in `compose.rs` without offers | No abstraction |
| Suggest auto-close on short terminal | Drops user mid-`/` without policy in PRD |
| Using `SurfaceIntent::Dock` to host Todos | Wrong layer (modal vs plane) |
| Dock Stack owning todo/guidance domain state | Wrong ownership |
| Painting overflow outside granted `Rect` | Layout violation |

## Migration plan

| Step | Work | Shipable alone? |
|------|------|-----------------|
| **M1** | Introduce `features/dock_stack` types + `solve` + tests | yes |
| **M2** | Wire Guidance + Suggest + Composer through offers/grants; delete ad-hoc heights in compose | yes (behavior fix for short terminals) |
| **M3** | `Region::Todos` + todos offers (0) wired | yes |
| **M4** | Real todos strip provider + paint | depends F-27 projection |

**Todos strip (M4) must not precede M1–M2.**

## Verification

- Solver table tests: body 20/30/50 × offer matrices.
- Shrink order: Suggest reduced before Todos below min; Guidance stays at 1.
- Compose: region order Todos → Suggest → Guidance → Composer when all granted.
- Modal: Suggest offer inactive.
- Integration: full stack preferred vs granted Stream height ≥ stream_min.
- Regression: idle frame contains resident Guidance directly above Composer.

## Related

| Doc | Role |
|-----|------|
| [features/dock-coexistence.md](../features/dock-coexistence.md) | Behavior contract + ASCII |
| [design/todo-list.md](./todo-list.md) | Band provider for Todos |
| [design/line-layout.md](./line-layout.md) | In-band text columns (orthogonal) |
| [design/shell-surface-layout.md](./shell-surface-layout.md) | Historical shell notes |
| navigation `compose.rs` | Thin consumer of grants |
