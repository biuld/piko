# Design: Modal Surfaces & Hitmap Architecture

> Status: draft
>
> PRD: [`../features/modal-surfaces-pointer-ready.md`](../features/modal-surfaces-pointer-ready.md)

## Goal

Make modal mounting, focus, and rendering obey one set of layout constraints,
and make the solved layout the single source of truth for future pointer
hit-testing. The design fixes the focus/modal divergence, makes Decide dialogs
opaque, and adds a derived hit-test contract without implementing mouse input.

## Principles

1. **One geometry authority.** `FramePlan<R>` (plane rects + ordered layer
   rects) produced by `solve()` is the only place rects are computed. Feature
   renderers receive their rect from the plan and never compute or draw
   outside it.
2. **One modal authority.** A single `modal_surface()` decides both what is
   drawn and what owns input. The focus stack must agree with it whenever a
   modal is open.
3. **Z-order is data.** Layers are an ordered list; paint order and hit
   priority both come from that order (last painted wins).
4. **Hit regions are derived, never duplicated.** The frame plan answers
   "which region owns (x, y)" from its solved rects; surfaces only declare
   sub-regions as offsets within their own rect.
5. **Engine stays product-agnostic.** `piko-tui-layout` keeps generic `R`/`T`;
   hit-testing is over region ids. Per-surface hit specs are product types in
   `piko-tui`.

## Compositing model (ratatui)

The design does **not** use a per-layer framebuffer. ratatui renders into one
global `Buffer` (a cell array); z-stacking is plain painter's algorithm:

```text
paint chrome → paint plane → paint layer 0 → paint layer 1 → …
```

Later paints overwrite earlier cells, and ratatui diffs the resulting buffer
against the previous frame, emitting changes for modified cells only. The
`FramePlan` layer order is the single source for both draw order and (future)
hit priority — the same data drives both, so they cannot disagree.

ratatui 0.30's relevant mechanisms:

- `Clear` widget — resets every cell in a rect. This is ratatui's official
  popup pattern (`examples/popup.rs`) and is the layer-boundary primitive the
  Decide backdrop uses.
- `Buffer::merge` — blits a whole off-screen `Buffer` over the target. This is
  the closest thing to framebuffer compositing, but it is **not needed** here:
  our layers paint sequentially into the one buffer, with no per-layer
  caching, occlusion culling, or selective redraw that would justify extra
  allocation.
- No built-in `ZStack`/layer widget exists in ratatui 0.30; `Cell`'s
  `MergeStrategy` (`Exact`/`Fuzzy`) only handles box-drawing glyph joins, not
  z-compositing.

Hit-testing is independent of compositing: it is a pure function of solved
rects, so neither the framebuffer question nor cell diffing affects the hit
contract. Even if per-layer buffers were used for compositing, hit-testing
would still be geometric: a merged buffer holds only the final glyph per cell
and loses layer provenance, so reading it cannot answer "which layer/element
owns this cell" without a separate per-cell ownership table — strictly worse
than rect math. Framebuffers solve "what is drawn"; rects solve "what owns a
coordinate".

## Current flow and gaps

```text
compose_frame(app)
  → resolve_modal_surface()      // draw authority: host-priority then focus
  → compose_plane + compose_modals  // at most ONE modal layer
  → solve(body, plane, modals)   // FramePlan<R>
  → render: chrome → plane (unless Browse) → layers

route_key(app, keymap, key)
  → P1 global Esc/Enter
  → P1.5 global shortcuts (F4 = AgentPanel)   // bypasses surface capture
  → P2 focus owner (focus_manager.active())
  → P3 editor
```

Gaps found in the current implementation:

- `resolve_modal_surface` (draw) and `focus_manager.active_mode()` (input) can
  diverge. Reproducer: F4 while an Approval is pending pushes `Agents` onto
  the focus stack, while `resolve_modal_surface` still returns Approval. Keys
  then route to an invisible Agents surface.
- Decide panels render with only a top border and a paragraph; the rest of the
  centered host rect shows background cells and can carry stale glyphs.
- `FramePlan` exposes rects for painting but has no way to answer a coordinate
  query, so there is no foundation for hit-testing.
- Approval letter shortcuts match `KeyCode::Char('a'|'w'|'p')` without checking
  modifiers, so Ctrl-modified chords can accidentally grant approvals.
- Help lines advertise ↑/↓ navigation that the Approval surface does not route.

## Architecture

### 1. Modal authority (`piko-tui`)

Replace the ad-hoc `resolve_modal_surface` + focus convention with one
authority on `AppState`:

```rust
impl AppState {
    /// The surface that owns BOTH drawing and input right now.
    fn modal_surface(&self) -> Option<SurfaceId>;
}
```

Rules:

- Selection order stays host-priority: Approval (non-empty queue or pending
  submission) → Tool Interaction → `focus_manager.active().as_surface()`.
- **Invariant**: when `modal_surface()` is `Some(s)`, the focus stack top is
  `s`. Enforced by construction:
  - Host-priority pushes (Approval / Tool Interaction events) push onto the
    stack and become the new top.
  - Non-host pushes (`push_surface` from shortcuts or commands) are **rejected
    while a Decide surface is pending**. Instead of opening a second surface,
    the shortcut is ignored (or queued as a later open, out of scope).
  - Pending-submission state (response sent, hostd not yet resolved) keeps the
    surface as both drawn and focused; input is read-only for resubmit.
- The F4 `AgentPanel` shortcut in the P1.5 slot must consult
  `modal_surface()` before opening Agents, closing the only current bypass.

### 2. Layout: one solve, derived hit-test (engine)

Extend `FramePlan<R>` in `piko-tui-layout` with a pure hit-test over solved
rects:

```rust
pub struct Hit<R> {
    pub region: R,
    /// `None` = plane; `Some(i)` = layer index (top-most wins).
    pub layer: Option<usize>,
    pub rect: Rect,
}

impl<R: Copy + Eq + Hash> FramePlan<R> {
    /// Top-most region owning the cell, or None when the cell is chrome-free
    /// space (outside body). Ratatui cell semantics: x in [r.x, r.x+r.width).
    pub fn hit_test(&self, x: u16, y: u16) -> Option<Hit<R>>;
}
```

Behavior:

- Scan layers in reverse solve order (last painted = highest priority), then
  the plane rects.
- Each `Region::Surface(_)` rect maps to `Hit { region, layer: Some(i) }`;
  plane rects map to `layer: None`.
- `hit_test` is a pure function of `FramePlan`; no product types, no input
  state. Unit tests use dummy region enums (existing crate policy).

This gives the future pointer-input feature a stable entry point:

```text
mouse event (x, y)
  → frame.hit_test(x, y)
  → layer surface?  → per-surface sub-region spec → surface action
  → plane region?   → stream scroll / composer focus / notification
```

### 3. Per-surface hit specs (`piko-tui`, future mouse PRD)

Each surface may declare interactive sub-regions **relative to its solved
rect**, produced by the same layout code that paints:

```rust
pub struct SurfaceHitSpec {
    pub surface: SurfaceId,
    /// (stable id, rect within the surface rect)
    pub items: Vec<(HitId, Rect)>,
}
```

Examples: choice rows and tabs in `InteractiveWorkflow`, list rows in
selectable panels, the Submit confirm row. The renderer already knows these
rows; the spec is the same geometry exposed as data. Hit resolution composes
the frame-plan hit with the surface spec:

```rust
fn resolve_pointer(
    plan: &FramePlan<Region>,
    surface: SurfaceId,
    surface_spec: &SurfaceHitSpec,
    x: u16, y: u16,
) -> Option<PointerTarget>; // e.g. Choice { question, choice } | Tab | Submit
```

The pointer-input PRD later maps `PointerTarget` to the same actions the
keyboard router produces. This design only fixes the geometry/data contract.

### 4. Rendering contract (`piko-tui`)

- `render_surface` receives its rect exclusively from
  `plan.rects` / `layer.rects`; no surface computes rects.
- Modal renderers must `Clear` their host rect before painting. Decide
  surfaces additionally fill the host with the theme background so the dialog
  is opaque (no stream/composer/stale glyphs inside).
- `InteractiveWorkflow` becomes the shared opaque panel: border + backdrop +
  content; Approval keeps its decision row and shortcut help inside the same
  panel, and the generic help line is removed or made truthful per surface.
- Letter shortcuts in the Approval router check
  `!modifiers.contains(CONTROL|ALT)`.

### 5. Focus stack changes (`piko-tui`)

- `FocusManager<T>` stays as-is (generic LIFO); the authority moves to
  `AppState::modal_surface()` + the push guard.
- `pop_focus` after a Decide resolves returns to the previous surface; if
  another Decide prompt is queued (Approval → Tool Interaction), it is pushed
  immediately so the invariant holds on the next frame.

## Hit granularity

Two tiers, deliberately **not** a full component-tree hit test:

### Tier 1 — z-axis hit (engine, always on)

`FramePlan::hit_test(x, y)` → `Hit<R>`: which plane region or modal surface
owns the cell, respecting z-order (layers top-down, then plane). This answers
"which surface is under the pointer" and is the mandatory first step for any
pointer event.

### Tier 2 — interactive-element hitmap (product, per surface)

When the z-hit lands on a surface, that surface resolves the coordinate
against its `SurfaceHitSpec`: a **flat list** of `(HitId, Rect)` covering
interactive affordances only:

- choice rows, question tabs, and the Submit/Confirm row in workflows;
- list rows in selectable panels;
- pane chrome actions (close/back/filter) and toggle rows in settings;
- the Composer resolves at region level; cursor placement computes the column
  directly from `x` (no prebuilt per-cell hitmap).

Non-interactive cells (text, borders, padding) produce no individual hits;
they resolve to the surface's default action or a no-op.

**Granularity rule: a hit region exists iff it maps to a distinct action.**
Nothing finer than a row/element is registered. Text selection or span-level
behavior is computed on demand from the same geometry, never pre-enumerated.

### Unified hitmap (z derived from layer order)

Tier 1 and Tier 2 collapse into **one flat list per frame** when every surface
host rect is itself a hit region:

```rust
pub struct HitRegion {
    pub rect: Rect,
    pub z: u16,               // derived: plane = 0, layer i = i + 1
    pub surface: SurfaceId,
    pub element: Option<HitId>, // None = surface background / default action
}

pub struct HitMap {
    /// Built each frame from FramePlan layers + per-surface element specs.
    pub regions: Vec<HitRegion>,
}
```

- `z` is **never hand-maintained**: plane = 0, layer index + 1, recomputed
  from the same solve that produced the rects.
- Surface-level regions (whole host rect, `element: None`) are mandatory: a
  click on a modal's empty or border cell must resolve to that surface's
  default action, **not** fall through to a lower layer.
- Elements inherit their surface's `z`. Within one surface, sub-regions do not
  overlap (flat rect list), so the only tie-break needed is declaration order;
  an element wins over its own surface background (same `z`, element first).
- Hit resolution: scan regions by `z` descending, first containing rect wins —
  one pass, O(regions).

Tier 2 therefore implies Tier 1 **only if** those surface-level entries exist;
the z-axis test is just the subset where `element.is_none()`.

Hit-testing follows **paint order (`z`)**, never the focused surface. The
focus-modal invariant makes the two agree while a modal is open, but z is the
authoritative axis because clicks target what is drawn on top.

### Component base trait (unified base for all components)

All drawable/hittable pieces share one base trait in `piko-tui-layout` — the
Rust analog of a parent class (composition over inheritance):

```rust
/// Unified component base: paint + own interactive regions.
/// No region id: it is assigned by the container that owns the component.
pub trait Component<E, C: ?Sized> {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &C);
    /// Same geometry as `render`; `Vec::new()` = not interactive.
    fn hit_regions(&self, area: Rect) -> Vec<HitRegion<E>>;
    /// Optional now; enables pointer-focus routing on the same base later.
    fn focusable(&self) -> bool {
        false
    }
}
```

- Implemented by leaf widgets (choice row, tab, button, list row) and
  composite panels alike. A panel's `render`/`hit_regions` delegates to its
  child components with their sub-areas; the hit map stays flat because
  `rebuild_hitmap` enumerates once per frame. The base trait is a shared
  vocabulary, **not** a recursive point-query mechanism.
- The base excludes z (assigned by the solver/container), region id (assigned
  by the owning region), and actions (product-defined).
- `FramePlan::hit_test` stays a plain generic method (no trait): region-level
  z-hit is a rect lookup, not behavior.
- Naming: `Component` avoids ratatui's `Widget` (stateless paint only) and
  `Clickable` (input-modality-specific). The contract is paint + geometry +
  element identity, so hover/drag reuse it later.

### Surface encapsulation in the layout crate

"Application layer" here means the crate directly above ratatui:
`piko-tui-layout`. The surface contract lives there, fully generic over
product ids. It is the **region-level wrapper** over `Component`: it adds the
region id that components deliberately do not know:

```rust
// piko-tui-layout — generic; R = region id, E = element id, C = render context
pub struct HitRegion<R, E> {
    pub region: R,
    pub rect: Rect,
    pub element: Option<E>, // None = surface default action
}

pub trait SurfacePanel<R, E, C: ?Sized>: Component<E, C> {
    fn region(&self) -> R;

    /// Component hits stamped with this panel's region id, plus one
    /// `element: None` entry over the whole area (surface default action).
    fn hit_regions(&self, area: Rect) -> Vec<HitRegion<R, E>> {
        let id = self.region();
        Component::hit_regions(self, area)
            .into_iter()
            .map(|h| HitRegion { region: id, rect: h.rect, element: Some(h.element) })
            .chain(std::iter::once(HitRegion {
                region: id,
                rect: area,
                element: None,
            }))
            .collect()
    }
}
```

- `piko-tui` implements `SurfacePanel` once per region with `R = Region`,
  `E = HitId`, `C = Theme` (or a small render context). The existing
  `render_surface` match becomes thin calls into the trait, and `build_hitmap`
  merges layer z from `FramePlan` with `hit_regions` — one pass per frame.
- Implemented by modal regions (Approval, ToolInteraction, Tree, …) **and**
  plane regions (Stream → Timeline scroll/click, Composer → Editor focus +
  cursor column, Notice → dismiss); plane entries join the hit map at `z = 0`.
- The wrapper is the trait itself — no trait objects or dynamic registry.
  `Surface` never owns z, focus, or the frame plan; those stay in the engine
  pure functions.
- Crate-boundary consequences:
  - `piko-tui-layout` now defines a **paint contract** (render trait) in
    addition to geometry. Dependencies do not change: ratatui is already the
    only dep, and no `piko-*` types are referenced.
  - Nothing product-specific leaks: no `SurfaceId`, no theme types, no action
    enums. `C` is a generic context so themes and render state stay
    product-owned.
- Anti-drift rule: `hit_regions` must derive from the same layout computation
  as `render` (a shared `layout(area)` when a surface has non-trivial rows);
  acceptance tests compare hit rects against painted rows.

### No app-level facade needed

Component encapsulation moves behavior into components, but four concerns are
inherently cross-component and cannot live in any single component:

- **geometry assignment** — which rect each region/component gets (flex
  solve, modal placement, shell split) is decided top-down;
- **z-order** — which layer wins a coordinate;
- **the focus stack** — a LIFO stack is not a component property;
- **hitmap assembly** — merging `FramePlan` z with each region's
  `hit_regions` into one flat map.

These need an orchestrator, but `piko-tui` already has one: `AppState` owns
`FocusManager`, every panel, and `mode`. A separate `LayoutApp` type would
duplicate focus ownership and add indirection, so it is **dropped**:

- `piko-tui-layout` stays a set of pure functions and data: `solve`,
  `FramePlan`, `build_hitmap`, `HitMap`, `hit_test`, `FocusManager<T>`,
  `Component<E, C>`, `SurfacePanel<R, E, C>`.
- `AppState` is the composition root. Each frame it:
  1. composes plane/modals (product policy, unchanged);
  2. calls `solve(body, plane, modals)`;
  3. calls `build_hitmap(&plan, panels)` where `panels` maps each region to
     its `SurfacePanel` impl;
  4. paints chrome → plane → layers by walking the plan and calling each
     region's `render`.
- Product policy (modal authority, push guards, key routing, dispatch) stays
  in `AppState` methods as today; `FocusManager` remains on `AppState`.

The "app-level API" therefore shrinks to a few call sites in `piko-tui`'s
`layout/mod.rs` and `render/mod.rs`; the engine exposes no facade type.

### Why not a component-tree hit test

Terminal layout is a flat 2D rect stack, not a DOM: siblings do not overlap
(flex prevents it), only modal layers do (Tier 1 already resolves that), and
scrolling is contained inside each surface's own viewport. A recursive
component hit tree would add traversal state without resolving anything the
two tiers cannot.

## Validation

Unit / integration tests:

1. `piko-tui` focus-modal invariant: with a pending Approval, pressing F4 does
   not change `modal_surface()` or the focus top (regression for the bypass).
2. `piko-tui-layout` `hit_test`: layer over plane, top-most layer wins, edge
   coordinates (`x == r.x`, `x == r.x + r.width - 1`), and no-hit outside
   body. Uses dummy region enums.
3. `piko-tui` rendering: after opening a Decide surface, cells inside the host
   rect outside the panel text carry the backdrop fill (snapshot/diff test).
4. Approval router: Ctrl-modified `a`/`w`/`p` do not resolve decisions.
5. Existing `layout`/`compose`/`focus` suites stay green.

Run before merge:

```text
cargo fmt --all
cargo test -p piko-tui-layout
cargo test -p piko-tui
cargo clippy --workspace --all-targets -- -D warnings
```

## Milestones

1. **Docs** — this PRD + design (current step).
2. **Modal authority** — `AppState::modal_surface()`, push guard, F4 fix
   (`piko-tui`).
3. **Opaque Decide** — `Clear` + backdrop in `InteractiveWorkflow`, truthful
   help lines, Approval modifier check (`piko-tui`).
4. **Engine hit-test** — `FramePlan::hit_test` + tests (`piko-tui-layout`).
5. **Surface hit specs** — expose render geometry as `SurfaceHitSpec`
   (`piko-tui`); then the pointer-input PRD consumes it.

## Non-goals (this design)

- Mouse event handling, hover, drag, click dispatch.
- Multiple simultaneous modals.
- Backdrop dim styling.
- Changes to the flex solver or focus stack internals.

## Implementation status

Landed in this pass:

- `piko-tui-layout/src/hitmap.rs`: `Component<E, C>`, `SurfacePanel<R, E, C>`,
  `HitRegion`, `Hit`, `HitMap`, `build_hitmap`, plus
  `FramePlan::hit_test` (15 engine tests, dummy enums).
- `piko-tui`: modal authority (`AppState::modal_surface` / `pending_decide`),
  Decide push guard, F4 barrier in the router, `surfaced` flag so
  auto-resolving interactions never become a visible barrier, and
  `InteractionEvent::Resolved` pops focus instead of clearing it.
- `piko-tui`: `InteractiveWorkflow` completion — ↑/↓ choose, Tab/Shift+Tab
  steps, truthful registry-derived Guidance, Ctrl/Alt modifier
  checks on approval letters, submit/cancel input lock, opaque backdrop
  (`Clear` + theme background), and `component_regions` exposing tabs /
  choices / submit as hit data.
- `piko-tui`: `ApprovalPanel` and `ToolInteractionPanel` implement
  `SurfacePanel`; `build_surface_hitmap` derives the per-frame hit map from
  the solved plan (integration test asserts modal-over-plane z + choice rows).
- `piko-tui`: every surface panel now implements `Component` / `SurfacePanel`
  (Sessions, Tree, Settings, Models, Thinking, Status, Diagnostics, Mcp,
  AuthSelector, Agents — each with its own render context type), and
  `render_surface` is a thin match of trait calls. `build_surface_hitmap`
  covers all modal surfaces; plane regions still contribute no hit specs.
- `piko-tui`: Decide modals render through the shared pane chrome
  (`PaneSpec::fill` backdrop, `PaneTitleAffix::Label`, hint footer), and
  `PaneSpec::content_rect` is the single geometry source for paint and hits.

Follow-ups that landed afterwards:

- Plane hit specs (Stream, Composer, Notice, Suggest) and the pointer-input
  feature (click / hover / wheel dispatch) — see
  [`pointer-input.md`](pointer-input.md) and its PRD.
- Drag, hover styling, and per-element actions for Browse/Select surfaces
  remain future work.
