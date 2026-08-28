# Design: Timeline Thought Inspector

> Status: implemented
>
> PRD: [../features/timeline-thought-inspector.md](../features/timeline-thought-inspector.md)

## Goal

Replace inline, wrapping thinking bodies with stable one-row Timeline
components and add a centered live inspector. Paint and pointer routing must
continue to share one content-space layout plan, and opening the modal must not
pause or snapshot the background Timeline.

## Ownership and data flow

```text
orchd model event clock
  ├─ realtime thinking deltas ───────────────────────────────┐
  └─ committed Thinking { text, duration_ms }                │
                                                            v
hostd stream + journal → piko-client-core AgentTimeline projection
                                      │
                                      v
                         TUI ThoughtComponent projection
                           ├─ one-row Timeline paint + hit
                           └─ ThoughtInspector by stable key
                                      │
                           normal tick│live delta
                                      v
                        repaint plane, then centered modal
```

- `orchd` measures model-runtime thought segments and supplies final duration.
- `hostd` transports and persists committed assistant content without creating
  a TUI-specific durable state path.
- `piko-client-core` remains the only reducer of committed and realtime
  Timeline data.
- `piko-tui` owns one-row presentation, local live elapsed display,
  typewriter reveal state, viewport state, and modal focus.

## Protocol and runtime contract

Extend the existing thinking content block with optional final timing:

```rust
ContentBlock::Thinking {
    thinking: String,
    thinking_signature: Option<String>,
    duration_ms: Option<u64>,
}
```

The field is optional for wire and storage compatibility. New committed model
output writes it; legacy blocks decode as `None`. This remains content metadata,
not a new session entry or read model, so the append-only journal and session
schema do not gain a second authority.

`AssistantMessageState` in orchd tracks a monotonic start for each ordered
thinking run. It closes the run when arrival order moves to a non-thinking
block or the model step finishes, and writes the elapsed duration into the
corresponding final block. Contiguous reasoning deltas extend the same run.
Tests use the existing clock abstraction or an injected monotonic clock; wall
clock subtraction is not used for elapsed time.

Realtime transport does not need to send a duration on every delta. The TUI
starts an approximate monotonic timer when the canonical projection first
exposes a non-empty live segment, freezes it on the same segment transition,
and replaces the approximation with committed `duration_ms` when available.
The committed value is the value replayed after reopening a session.

The current orchd collector hard-codes thinking `content_index: 0`. It must
instead increment the thinking content index whenever a non-thinking block
closes a run and thinking later resumes. A new index makes the resumed run a
new client-core segment in arrival order; contiguous deltas retain the current
index. This is required for live `thought → text/tool → thought` order to match
the committed message. The committed projection derives the same identity from
the thinking-run ordinal, so no stream-only index needs to become durable
transcript data.

## Timeline model

Thinking becomes a first-class Timeline presentation component instead of a
body variant rendered inside `AssistantMessageComponent`:

```rust
pub struct ThoughtKey {
    message_id: String,
    segment_index: u32,
}

pub enum ThoughtPhase {
    Streaming { observed_at: Instant },
    Completed { duration_ms: Option<u64> },
}

pub struct ThoughtComponent {
    key: ThoughtKey,
    text: String,
    phase: ThoughtPhase,
}

pub enum TimelineComponent {
    // existing variants ...
    Thought(ThoughtComponent),
}
```

Projection splits ordered assistant content into assistant text/image runs,
thought rows, and interleaved tool cards. `segment_index` is the live thinking
`content_index`; for committed content it is derived from the run's zero-based
ordinal among thinking blocks. Realtime-to-committed reconciliation keeps
`(message_id, segment_index)` stable. If a malformed final message cannot
reconcile that key, the old key is retired; it is never reassigned to a
different thought.

Timeline interns each `ThoughtKey` to a monotonic local `u64`, parallel to tool
hit interning:

```rust
HitId::TimelineThought(u64)
```

Interned ids are not reused after clear or rebuild. The inspector stores the
semantic `ThoughtKey`, while a pointer action carries the short interned id and
resolves it at reduction time.

## One-row rendering

`thought_lines` always returns one `Line` for a visible component. It uses the
shared column truncation helpers rather than wrapping. Duration formatting is
shared by summary and inspector chrome so a transition cannot display two
different rounded values.

The thought spinner is a second named feedback family, for example the
quadrant cycle `◐ ◓ ◑ ◒`; the Bottom Bar retains its braille cycle. A completed
thought replaces the animated frame with the shared success glyph `✓`. Keeping
the families explicit in the feedback component avoids an accidental future
reuse while allowing both to advance from `AppState::spinner_frame`.

Hover is an input to thought line rendering and changes semantic style only;
it does not mutate the component or viewport. The line cache key therefore
includes whether this particular thought id is hovered.

## Content-space hits and selection

Generalize the current Timeline row owner:

```rust
enum RowOwner {
    Tool(u64),
    Thought(u64),
}
```

While flattening component lines, `Timeline::render_plan` records the exact
content row emitted by every `ThoughtComponent`. `ContentHitPlan` continues to
translate the current live viewport offset at event time, so repaint-time
screen rectangles are not cached as interaction authority.

`TimelineRenderPlan::resolve` maps a thought owner to
`HitId::TimelineThought`. Hover reconciliation uses the same resolver after
keyboard or wheel scrolling.

The existing stream pointer path begins selection on left-down and decides
activation on release. Replace its tool-only activation payload with a closed
Timeline activation enum:

```rust
enum TimelineActivation {
    Tool(u64),
    Thought(u64),
}
```

A release without movement activates the resolved owner. Drag/update/finish
continues selection and suppresses activation. Wheel events remain Timeline
scroll actions regardless of row ownership.

## Inspector surface

Add `SurfaceId::ThoughtInspector` with:

- `SurfaceSizing::Centered(ThoughtContent)`
- `SurfaceInputProfile::ReadOnlyViewport`
- `OutsideClickPolicy::Dismiss`
- no resident guidance row

Centered placement is required: `CoverBody` intentionally skips plane paint,
while this feature must show live Timeline changes behind the inspector. The
existing composition already paints the plane before centered modal layers and
the modal barrier already prevents pointer fall-through.

`AppState` retains `Option<ThoughtInspectorState>` containing the active
`ThoughtKey`, inspector viewport, reveal cursor, and last reveal tick. It does
not copy the thought text. Render resolves the key against the selected
agent's current Timeline on every frame. Completion therefore changes the open
surface in place, and projection reconciliation cannot leave it showing stale
content. If the key is genuinely removed, the surface closes on the next
update.

Opening the inspector primes the reveal cursor to the grapheme count already
received. Later live content advances a target length; ticks reveal a bounded
number of `unicode-segmentation` grapheme clusters and paint a cursor while the
target is ahead. On completion, any buffered tail is revealed and the cursor
is removed. Completed thoughts skip reveal state and render all content.

The inspector owns a wrapped-line `ScrollViewport`. Its metrics are recomputed
as revealed content grows. If at latest it remains pinned; if the user scrolls
up, new revealed lines do not move the view unexpectedly.

## Background streaming contract

No event queue or projection path is conditional on modal focus. Host batches,
ticks, `Timeline::sync_projection`, viewport metrics, and pending-new tracking
continue normally. `prepare_frame` continues preparing the Timeline plan for a
centered modal, and `render_prepared` paints:

1. Bottom Bar.
2. Chat plane, including the current Timeline plan.
3. Centered thought inspector layer.

The plane is non-interactive while any modal is active, so background hover is
suppressed even though live paint continues.

## Change map

- `packages/protocol/src/messages.rs`: optional committed thought duration.
- `packages/orchd/src/runtime/events/delta_lane.rs`: segment timing and final
  metadata.
- `packages/client-core/src/timeline/`: preserve ordered thought identity and
  committed timing through reconciliation.
- `packages/tui/src/features/timeline/`: thought component projection, fixed
  row presenter, stable hits, activation, and tests.
- `packages/tui/src/features/thought_inspector/`: modal state, wrap/viewport,
  typewriter reveal, panel, and tests.
- `packages/tui/src/navigation/`, `layout/`, `render/`, and `app/dispatch/`:
  surface catalog and composition wiring only.
- theme resources: semantic thought-row hover and inspector tokens only if an
  existing interactive-row/pane token cannot express the states.

No `.rs` file should cross the 500-line ceiling; projection and inspector
state/render tests should remain in their existing split module structure.

## Verification

- Protocol serde tests cover missing and present `duration_ms`.
- Orchd clock-controlled tests cover contiguous deltas, interruption by text
  or tool activity, message end, cancellation, and multiple thought runs.
- Client-core tests cover stable segment order and draft-to-commit timing
  reconciliation.
- Timeline render tests assert exactly one line at wide and narrow widths,
  distinct spinner frames, completed labels, and hover styling.
- Layout/pointer tests cover live-scroll hit resolution, non-overlap with tool
  hits, stable identity across rebuild, stale-id no-op, click activation, drag
  suppression, and hover after viewport changes.
- Inspector tests cover late open, grapheme-safe reveal, completion while open,
  internal scroll pinning, missing-key close, Esc/outside dismiss, and modal
  input barrier.
- A render integration test applies live deltas while the centered inspector is
  active and asserts that both the background Timeline row and foreground body
  advance in the same frame sequence.

Before implementation is considered complete:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
