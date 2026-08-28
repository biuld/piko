# Timeline Thought Inspector

> Status: implemented
>
> Parent: [timeline.md](timeline.md)
>
> Design: [timeline-thought-inspector.md](../design/timeline-thought-inspector.md)

## Overview

Assistant thinking is represented in Timeline by a compact, actionable summary
row. A live thought occupies one visual row while its content streams; a
completed thought keeps the same one-row footprint. Activating either state
opens a centered inspector containing the thought text.

The feature keeps the conversation stream geometrically calm while preserving
access to the complete thought. It does not move turn ownership or stream
reduction into the TUI: hostd remains authoritative for committed content and
the canonical client projection remains authoritative for live content.

## Terms

A **thought** is one ordered thinking segment in an assistant message. Most
messages contain one segment. If thinking is interrupted by text or an
interleaved tool and later resumes, each segment keeps its chronological place
and receives its own summary row.

The **thought duration** starts with the first content in that thinking segment
and ends when the segment closes. A later non-thinking segment, message end,
failure, or cancellation closes an open segment. Live elapsed time uses a
monotonic clock; the committed duration is the durable authority after
reconciliation.

## Layout

Timeline always allocates exactly one visual row for each visible thought:

```text
live       ◐ thinking... (2.4s)
completed  ✓ thought in 2.4s
```

- The live spinner uses a visually distinct animation from the Bottom Bar
  agent spinner.
- The completed state uses the shared success glyph as the spinner's static
  completion state.
- The row never soft-wraps. On narrow terminals, content is truncated while
  preserving the state and duration when space permits.
- The whole painted row is the pointer target, not only the label.
- Thinking style remains quieter than assistant answer text. Hover applies a
  visible background or foreground change using semantic theme tokens.
- Switching from live to completed changes text and animation, not row height.

The inspector is a centered modal over the normal Chat plane:

```text
┌──────────────── Timeline continues streaming ────────────────┐
│                                                              │
│          ┌──────────── Thought · 2.4s ────────────┐           │
│          │ complete or progressively revealed     │           │
│          │ thought content                         │           │
│          │                                         │           │
│          │ Esc close                     scroll ↕  │           │
│          └─────────────────────────────────────────┘           │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

The modal must not cover the entire body. The Timeline remains visible behind
it and is repainted as stream updates arrive.

## Behavior / interactions

### Summary row

- The row appears when the first non-empty thinking content is projected.
- While live, the spinner and elapsed label advance on the normal TUI tick even
  when no new model delta arrives.
- When the segment closes, the row changes in place to `thought in <duration>`
  and freezes its duration.
- Failure and cancellation still finalize an already visible row; they do not
  discard received thought content.
- Newly committed thoughts retain their duration across session reload and
  agent switching. Legacy entries without duration metadata use `thought`
  rather than inventing a time.
- When Timeline thinking is hidden by configuration, neither the summary row
  nor its inspector activation is exposed.

### Pointer behavior

- Moving over either state provides hover feedback and does not change
  Timeline selection.
- A left click without a drag opens the matching thought.
- A drag continues to mean text selection and must not open the inspector on
  release.
- Scrolling over the row continues to scroll Timeline.
- A stale pointer identity after projection replacement is a no-op; it must
  never open a different thought.

### Inspector

- Opening a completed thought shows all content immediately.
- Opening a live thought shows content already received immediately. Content
  received after opening is revealed by grapheme cluster with a typewriter
  cursor on TUI ticks. The reveal may catch up in bounded batches, but it must
  never split a user-perceived character or reorder content.
- If a live thought completes while open, the same inspector transitions to
  completed state in place and reveals any remaining buffered content.
- The inspector resolves content from the current canonical projection by
  stable thought identity; it does not retain a click-time text snapshot.
- Long content soft-wraps and scrolls inside the inspector. Wheel, PageUp,
  PageDown, Up, and Down scroll the inspector while it owns focus.
- `Esc` closes the inspector. Clicking outside follows ordinary dismissible
  modal behavior. Input never falls through to Timeline while the modal is
  open.
- Closing the inspector does not alter Timeline scroll position or thought
  state.
- Background Timeline streaming, pin-to-latest behavior, pending-new counts,
  elapsed timers, and projection reconciliation continue while the inspector
  is open.

## Configuration

The feature adds no setting or key binding. Existing Timeline thinking
visibility controls whether thought summary rows are present. Typewriter speed,
spinner family, and modal sizing are product presentation constants in the
first version.

## Acceptance criteria

- Every visible live or completed thought occupies exactly one Timeline row at
  all supported widths.
- Live and completed rows are actionable and have paint-aligned hover feedback.
- The thought spinner is not the Bottom Bar agent spinner animation.
- Clicking a row opens the matching content; dragging selects text and does not
  activate it.
- Live inspector content grows with a grapheme-safe typewriter effect.
- Completion updates both row and open inspector without closing or replacing
  the modal.
- Timeline continues to receive, lay out, scroll according to its existing
  viewport policy, and repaint behind the centered inspector.
- Scroll changes and projection rebuilds cannot retarget a thought hit.
- Committed duration survives snapshot replay for newly written entries.

## Non-goals

- Editing, copying through a dedicated button, or exporting thought content.
- Persisting whether the inspector is open or its scroll offset.
- Making thought rows part of keyboard Timeline navigation in the first
  version.
- Replaying already received live content from the beginning when the inspector
  is opened late.
- Backfilling exact duration for legacy transcript entries that never recorded
  it.
