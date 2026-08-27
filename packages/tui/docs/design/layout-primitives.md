# Design: TUI layout primitives

> Status: accepted (foundation, Diagnostics, Editor, and Timeline landed;
> generic live-hit registry and list migration deferred)
>
> Feature: [layout-primitives.md](../features/layout-primitives.md)

## Goal

Introduce a small foundation for local terminal layout, visual separation,
text wrapping, viewports, and pointer geometry without duplicating the existing
shell/flex engine or moving product behavior out of `piko-tui`.

The decisive design rule is:

> Prepare geometry once; paint and hit resolution consume that same plan.

Timeline's content-row ownership and live viewport resolver are the reference
model. The design generalizes that model to Pane, Editor, selectable lists, and
read-only scrollable bodies.

## Implementation status

The current landing includes the typed geometry, text, viewport, Pane, and
content-hit primitives, plus the first consumers: Diagnostics, Editor, and
Timeline. Existing static hitmap routing and the typed
`PreparedFrame.timeline` slot remain as compatibility seams. A generic
`PreparedFrame.live_hits` registry and selectable-list migration are retained
as follow-up design work; the current consumers do not require erasing their
different paint plans behind one runtime type.

## Current shape and pressure

The workspace already has the correct major layers:

- `piko-tui-layout` owns product-neutral shell, flex, modal, focus, component
  interaction vocabulary, and the static hitmap.
- `piko-tui` owns product composition, theme paint, Pane, line layout,
  Timeline, Editor, and feature action mapping.
- `PreparedFrame` retains frame geometry and the Timeline render plan for
  pointer routing.

The missing foundation appears as repeated local implementations:

- Timeline and Editor both use bottom-origin viewport arithmetic and their own
  scrollbar mapping.
- Diagnostics delegates wrapping to `Paragraph` but clamps scrolling against
  unwrapped logical lines.
- Notifications, Todos, Usage, and selectable surfaces retain separate window
  and maximum-scroll state.
- `ui::line_wrap` returns painted lines while Editor separately computes source
  ranges for cursor mapping and atomic references.
- Pane can compute content areas and affix hits, but paint and consumers do not
  yet share an explicit retained Pane plan.
- `PreparedFrame.timeline` is a feature-specific instance of a pattern needed
  by all scrollable interactive content.

This is an extraction and contract-alignment change, not a new rendering
framework.

## Crate boundary

### `piko-tui-layout`

Add only product-neutral geometry and interaction math:

| Module | Responsibility |
|--------|----------------|
| `padding` | Four-sided saturating inset, spacer, gutter, clip, and alignment helpers. |
| `divider` | Local two-child split plus optional divider band. |
| `viewport` | Row-window state, anchors, metrics, visible range, scrollbar math. |
| `content_hit` | Generic content-space row/fragment ownership and resolution. |

These modules may use Ratatui geometry types and generic element ids. They must
not depend on piko theme, `Region`, `HitId`, actions, feature state, or protocol
types.

The existing `flex` module remains the only recursive region splitter.
`divider` is a local two-child recipe, not a second flex tree.

### `piko-tui`

Add or reshape terminal-presentation components:

```text
src/ui/
  text_layout/
    mod.rs
    model.rs
    wrap.rs
    position.rs
  components/
    divider.rs
    scroll_view.rs
    pane.rs
    pane/
      render.rs
```

- `text_layout` owns the shared `TerminalTextPolicy`-based wrapping model and
  Ratatui styled-fragment adapter.
- `components::divider` paints a divider plan with semantic theme tokens.
- `components::scroll_view` paints prepared rows and scrollbar chrome.
- `components::pane` remains the product chrome component, but exposes a
  prepared plan shared by paint and hit generation.

No new crate is needed.

## Preparation model

Do not introduce one object-safe universal layout trait. Geometry, text, Pane,
and structured Timeline blocks have different inputs and useful outputs; a
boxed component tree would obscure those types and duplicate the existing flex
tree.

Instead, each reusable primitive exposes a typed prepare function and a typed
plan:

```rust
let pane = prepare_pane(area, &pane_spec);
let text = text_layout.prepare(source, pane.content.width, wrap_options);
let viewport = viewport.prepare(text.row_count(), pane.content);

paint_pane(frame, &pane, &pane_spec, theme);
paint_scroll_view(frame, &text, &viewport, theme);
```

Plans are immutable snapshots except for paint-only scratch state required by
Ratatui widgets. Product state and mutations remain outside plans.

## Geometry primitives

### Padding

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Padding {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Padding {
    pub fn apply(self, area: Rect) -> Rect;
}
```

`apply` uses saturating arithmetic and never returns a rectangle outside
`area`. Symmetric constructors are convenience only; the four-sided value is
canonical. The existing horizontal-inset helper can delegate to this type.

### Divider split

```rust
pub enum SplitAxis { Horizontal, Vertical }

pub struct DividerSplit {
    pub axis: SplitAxis,
    pub first: SplitSize,
    pub divider: u16,
}

pub struct DividerPlan {
    pub first: Rect,
    pub divider: Option<Rect>,
    pub second: Rect,
}
```

The solver clamps all three bands to the parent. A zero-cell divider produces
no divider rectangle. Initial sizing reuses the existing fixed/percent/grow
math where practical; it does not add nested divider nodes to `Node<R>`.

Divider paint lives in `piko-tui` because glyph and color are theme policy.
The divider has no hit owner until a consumer explicitly declares one.

## Pane as a foundation container

Pane stays in `piko-tui` because its modes, search presentation, title affixes,
hints, semantic theme, and degradation policy are product chrome. Its geometry
becomes explicit:

```rust
pub struct PanePlan {
    pub outer: Rect,
    pub frame: Rect,
    pub title: Option<Rect>,
    pub search: Option<Rect>,
    pub search_rule: Option<Rect>,
    pub content: Rect,
    pub tip: Option<Rect>,
    pub footer: Option<Rect>,
    pub clip: Rect,
    pub affix_hits: Vec<(Rect, PaneAffixHit)>,
}

pub fn prepare_pane(area: Rect, spec: &PaneSpec<'_>) -> Option<PanePlan>;
pub fn paint_pane(
    frame: &mut Frame<'_>,
    plan: &PanePlan,
    spec: &PaneSpec<'_>,
    theme: &Theme,
);
```

`PaneAreas` becomes either a compatibility view over `PanePlan` during
migration or is removed after all callers consume the plan.

Pane interaction rules:

- affix hits are calculated during `prepare_pane` from the same title layout
  that paint uses;
- non-interactive title text, border, padding, and footer gaps remain Pane
  background;
- callers add child hits only inside `plan.content`, clipped to `plan.clip`;
- Pane never stamps a product `Region`, maps a hit to an `Action`, or changes
  focus;
- scrolling is composed inside `plan.content`, not added to `PaneSpec`.

## Text layout

### Shared wrap input

The wrap kernel operates on runs rather than only strings or already-painted
lines:

```rust
pub struct TextRun<P> {
    pub text: String,
    pub payload: P,
    pub source: Option<Range<usize>>,
    pub breakability: Breakability,
}

pub enum Breakability {
    Grapheme,
    Atomic,
    HardBreak,
}

pub struct VisualFragment<P> {
    pub text: String,
    pub cols: Range<u16>,
    pub payload: P,
    pub source: Option<Range<usize>>,
}

pub struct VisualLine<P> {
    pub fragments: Vec<VisualFragment<P>>,
    pub width: u16,
    pub hard_break: bool,
}

pub struct TextLayout<P> {
    pub lines: Vec<VisualLine<P>>,
    pub width: u16,
}
```

`P` carries style, semantic owner, or both. Implementations may borrow input
or intern repeated payloads after profiling; the first implementation should
prefer simple owned plans and correctness.

Two adapters feed the kernel:

1. plain/source text produces byte ranges and optional atomic reference runs;
2. Ratatui spans produce style payloads and optional stable owners.

Existing `soft_wrap`, `wrap_spans`, prefix/indent helpers, and Editor visual
line calculation migrate onto the kernel incrementally. Row-composition
recipes such as trailing timestamps remain above the wrap kernel.

### Position mapping

`TextLayout` exposes:

```rust
fn visual_position(&self, source: usize) -> VisualPosition;
fn source_position(&self, row: usize, col: u16, bias: PositionBias) -> usize;
```

Mappings clamp to valid grapheme boundaries. Atomic runs resolve only before
or after the atom. A glyph wider than the row budget is emitted whole on one
visual row and clipped during paint; it is never split at an invalid boundary.

## Viewport state and plan

Use top-origin state as the canonical representation. Bottom-following is an
explicit mode rather than inverted arithmetic in each feature.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewportMode {
    Fixed,
    FollowEnd,
}

pub struct ViewportState {
    top: usize,
    mode: ViewportMode,
}

pub struct ViewportMetrics {
    pub content_rows: usize,
    pub visible_rows: usize,
}

pub struct ViewportPlan {
    pub outer: Rect,
    pub content: Rect,
    pub gutter: Rect,
    pub visible: Range<usize>,
    pub scrollbar: Option<ScrollbarMetrics>,
}
```

State operations include:

- `update_metrics`, preserving the current mode;
- `scroll_by`, switching to `Fixed` unless it reaches and elects to follow the
  end;
- `scroll_to` and `scroll_to_fraction`;
- `follow_end`;
- `ensure_visible(target: Range<usize>)`;
- `max_scroll` and `visible_range`.

Consumers decide policy transitions. For example, Timeline clears pending-new
when it elects to follow the end; Editor resumes cursor follow after editing.
Those side effects are not callbacks inside `ViewportState`.

The viewport always reserves its configured gutter before text layout. The
scrollbar may be hidden when content fits, but the content width does not
change at the overflow threshold.

## Content-space hit plan

### Generic model

```rust
pub struct ContentHitPlan<E> {
    pub content_rect: Rect,
    pub clip_rect: Rect,
    pub rows: Vec<ContentHitRow<E>>,
    pub epoch: u64,
}

pub struct ContentHitRow<E> {
    pub fragments: Vec<ContentHitFragment<E>>,
}

pub struct ContentHitFragment<E> {
    pub cols: Range<u16>,
    pub owner: E,
    pub source: Option<Range<usize>>,
}

pub struct ResolvedContentHit<E> {
    pub owner: E,
    pub rect: Rect,
    pub source: Option<usize>,
}
```

The actual API should permit ownerless rows without allocating empty fragment
vectors for every row. Row-wide owners, used by Timeline tool titles and list
rows, get a compact representation. Fragment owners support wrapped links,
editor text positions, or future inline controls.

Resolution receives the current viewport state or top offset:

```rust
fn resolve(
    &self,
    viewport_top: usize,
    x: u16,
    y: u16,
) -> Option<ResolvedContentHit<E>>;
```

It rejects coordinates outside `content_rect` or `clip_rect`, including the
scrollbar gutter and reserved overlay rows. It then translates screen row to
content row and resolves the row-wide or fragment owner.

### Static gate and live resolver

The existing static `HitMap` remains the first stage:

```text
pointer
  -> HitMap: topmost Region / modal barrier
  -> region is static: return snapshot element
  -> region is live: ContentHitPlan.resolve(current viewport)
  -> product component maps owner + gesture to Action
```

This preserves one z-order and outside-modal authority. A live plan cannot
make content under a modal interactive.

`E` remains generic and cheap to clone or copy. Product integrations must use
stable semantic ids. Timeline's interned tool id remains valid; selectable
lists should use stable source ids rather than filtered/visible positions.

### PreparedFrame integration

The target architecture is a product-side live registry:

```rust
pub struct PreparedFrame {
    pub product: ProductFrame,
    pub hit_map: HitMap<Region, HitId>,
    pub live_hits: LiveHitRegistry,
}

pub enum LiveHitPlan {
    Timeline(TimelinePreparedPlan),
    Text(ContentHitPlan<HitId>),
    List(ContentHitPlan<HitId>),
}
```

The first implementation may use a typed enum because Timeline plans also own
paint lines and row metadata. Do not erase all plans behind `dyn Any` or force
unrelated content types into one struct. The registry unifies routing and
lifecycle, not product rendering data.

Each plan records a content-layout epoch. Pure scroll does not bump that epoch;
the live resolver reads the current viewport offset. Content, width, wrap,
theme inputs that affect geometry, expansion, and Pane zone changes bump or
replace the plan before event-time resolution. Existing input-batch refresh
behavior remains the compatibility requirement.

The current landing keeps `PreparedFrame.timeline` as the typed compatibility
slot. `TimelineRenderPlan` already contains the generic `ContentHitPlan`, so
its resolver follows the target content-space contract. The registry and
additional list plans should be introduced when a second interactive live plan
needs the same routing/lifecycle seam; they are not simulated by an unused
global registry in this slice.

## Component and hit composition

The existing `Component::component_regions(area)` API remains available during
migration, but new foundation components should prepare once:

```rust
let pane = prepare_pane(area, spec);
let child = prepare_child(pane.content, data);

let static_hits = pane
    .affix_hits
    .iter()
    .chain(child.static_hits())
    .map(|hit| clip_hit(hit, pane.clip));
```

Longer term, `Component` may gain an associated prepared-plan contract only
after two or more components demonstrate the same useful signature. This
design deliberately avoids changing the generic trait before those migrations.

## Paint adapters

`ScrollView` is a paint adapter over prepared rows and a `ViewportPlan`. It
owns no product data and performs no wrapping. It:

- paints only `visible` rows into `content`;
- applies the shared clip;
- paints the scrollbar when metrics request it;
- exposes track/thumb static hits only when enabled by its consumer;
- never converts an owner into a product action.

Pane, ScrollView, and Divider consume semantic theme inputs in `piko-tui`.
Their geometry plans remain theme-independent except when title/text widths
are explicit layout inputs.

## Migration sequence

### Slice 1: geometry and Pane plan — landed

- Add `Padding` and divider geometry to `piko-tui-layout`.
- Make the existing inset helper delegate to `Padding`.
- Split `render_pane` into `prepare_pane` and `paint_pane`.
- Keep `render_pane` as a compatibility wrapper while callers migrate.
- Make Pane affix hits consume `PanePlan` rather than recomputing title
  geometry.

### Slice 2: viewport and read-only body — landed for Diagnostics

- Add `ViewportState`, `ViewportPlan`, and scrollbar metrics.
- Add `ScrollView` paint.
- Migrate Diagnostics first so wrapped visual height becomes authoritative.
- Usage and Notifications remain possible follow-up consumers; Diagnostics is
  the current read-only validation surface.

### Slice 3: text layout and Editor — landed

- Add the run-based wrap kernel and position mapping.
- Adapt existing line-wrap helpers to it without changing Timeline appearance.
- Replace Editor's private visual-line calculation with source-mapped
  `TextLayout`.
- Keep Editor's cursor-follow policy in the product adapter while its window
  state and source-aware visual layout use the shared primitives.

### Slice 4: live-hit registry and lists — deferred

- Generalize `PreparedFrame` routing from the Timeline special case to typed
  live plans.
- Migrate selectable-list visible-row ownership and Diagnostics content hits.
- Preserve static hitmap modal gating and existing pointer action mapping.

### Slice 5: Timeline adapter — landed

- Retain Timeline line cache, block layout, stable tool ids, row ownership,
  pending-new behavior, and layout epoch.
- Replace only generic viewport/scrollbar/content-hit math.
- Remove the feature-private primitives after differential tests prove the
  prepared lines, visible window, and pointer targets are unchanged.

Todos remains an item strip with Dock Stack budgeting; migrate its window math
only if the shared state fits without importing text-view semantics.

## Current implementation map

| Area | Landed seam |
|------|-------------|
| `piko-tui-layout` | `padding`, `divider`, `viewport`, and `content_hit` modules; existing flex and hitmap remain unchanged |
| Pane | `prepare_pane`/`paint_pane` share `PanePlan` geometry and affix hits; `render_pane` remains a compatibility wrapper |
| Text | `ui::text_layout` wraps terminal graphemes and maps source boundaries; `ui::line_wrap` delegates to it |
| Diagnostics | shared `TextLayout`, `ViewportState`, reserved gutter, and `paint_scroll_view` |
| Editor | shared source-aware text layout with atomic reference runs and shared viewport state adapter |
| Timeline | shared viewport/scrollbar math and `ContentHitPlan<RowOwner>` with live top-offset resolution |

The remaining compatibility wrappers are deliberate. They preserve the
existing product-facing APIs while making the new prepared plans the geometry
source for migrated paths.

## Validation

### `piko-tui-layout` unit tests

- asymmetric padding and degenerate rectangles;
- horizontal/vertical divider clamping;
- viewport fixed/follow-end transitions, resizing, and ensure-visible;
- scrollbar edge mapping;
- content hit resolution with clipping, gutters, empty rows, and live offsets;
- local dummy ids only.

### `piko-tui` component tests

- Pane plan zones exactly match painted chrome and affix hits;
- child hits are clipped to Pane content;
- styled wrap, hard lines, CJK, emoji ZWJ, combining characters, and over-wide
  graphemes;
- source/visual mapping and editor atomic references;
- hidden scrollbar preserves the gutter and text wrapping width.

### Product integration tests

- modal z-order blocks every live content resolver below it;
- wheel-batch plus click resolves the post-scroll stable owner;
- hover reconciles through the same resolver;
- width/content/expansion changes refresh a stale plan once;
- pure scroll does not bump content-layout epoch;
- Timeline and Editor rendering snapshots remain stable unless an intentional
  visual change is separately approved.

## Decisions and tradeoffs

1. **Typed plans over a universal component tree.** This keeps source mapping,
   structured blocks, and Pane zones explicit.
2. **Top-origin canonical viewport state.** Follow-end is an explicit mode;
   product side effects stay with the consumer.
3. **Dual hit modes.** Static screen hits retain z-order authority; scrollable
   content resolves from content ownership and live viewport state.
4. **Pane is a composed foundation container in `piko-tui`.** Its local
   geometry uses shared primitives, while product chrome and theme stay out of
   `piko-tui-layout`.
5. **Text layout stays in `piko-tui`.** It depends on the terminal text policy,
   Ratatui styled text, and editor source mapping; Rect/window math stays in
   `piko-tui-layout`.
6. **Incremental adapters over a flag-day rewrite.** Existing Timeline
   validity behavior is preserved as the reference implementation.

## Failure containment

- Invalid or stale content owners resolve to component background, never a
  different item.
- Out-of-bounds source ranges are rejected in debug/tests and clamped or
  omitted in production plans; they never panic pointer routing.
- Zero-size or fully clipped plans paint nothing and expose no child hits.
- If a live plan is missing or stale and cannot be refreshed, routing falls
  back to the region default rather than stale content interaction.
- Foundation types carry no host or protocol state, so failure cannot mutate
  authoritative session data directly.
