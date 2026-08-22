# D-63: Assistant turn chips

> Status: design
> Feature: [F-46](../features/F-46-assistant-turn-chips.md)
> Amends: [D-62](D-62-timeline-conversation-blocks.md) row presentation; keeps D-62's virtualization seam and selection rules

## Model

An agent turn interleaves `think → toolcall → … → response → think`. The
timeline therefore groups consecutive **assistant-side** items into one
presentation unit and preserves segment order inside it:

```
AgentTimeline items            TimelineRow::Assistant { segments }
─────────────────────          ─────────────────────────────────
RealtimeDraft(think|text)  ┐
Tool(call-1)               ├─▶  [Think·spinner][Bash $ rg]
Committed(Assistant t1)    ┘    "Response paragraph…"        ← new line
Tool(call-2)                    [Read a.rs]
RealtimeDraft(think)            [Think·spinner]                ← trailing run
```

Grouping rule: extend the current group while items are `RealtimeDraft`,
`Comitted(Assistant)`, or `Tool`; any `User` / `Context` / `SessionEntry`
item closes it. Every item belongs to exactly one group; single-item groups
degenerate to today's rows (user chip, system label, or a chips-only bubble).

### Row types

```rust
enum TimelineRow {
    User { .. },                       // unchanged (F-44)
    System { .. },                     // unchanged
    Assistant {
        id: String,
        segments: Vec<TurnSegment>,    // chronological, adjacent-kind merged
    },
}

enum TurnSegment {
    Thinking { id, text, active },     // active = live draft tail
    Tool      { id, name, status },    // display only; body via lookup
    Text      { id, text, caret },     // caret on streaming tail
}
```

Adjacent same-kind blocks inside one message merge into a single segment so
multi-block model output does not spam chips. Tool payloads (`args`, result,
`partial_json`) leave the row; the overlay re-reads them from the projection
by `tool_call_id` when opened, keeping list paints clone-light.

## Virtualization (amends D-62 offsets)

`frame_timeline` now walks items once to build contiguous item-run **groups**;
each group renders exactly one list row, so `total()` is the group count and
row lookup is direct — no offset arithmetic at all.
Streaming flag unchanged (draft present or any tool running). `rows_around`
maps every item of the addressed group and merges segments — cost is
proportional to the visible turn, not the session. Parity tests pin grouping.

## Presentation

Bubble = existing assistant `ConversationBlock` surface (leading align, bot
icon, ElevatedChip, selectable). Body is a vertical stack built by splitting
segments on `Text` boundaries:

```
[chip-run]   flex-wrap row of ActivityChips (hug height)
[text]       markdown block (caret on streaming tail)
[chip-run] …
```

Trailing chips flush as their own row. A chips-only turn renders chip runs
inside an otherwise empty bubble.

`ActivityChip` (island, product-free): capsule with leading status slot +
label. Chip runs wrap via a plain flex-wrap row at the call site. Status slot renders one of: indeterminate `CircularProgress` (active),
static icon (done/stopped), danger icon + tint (failed). Thinking chips pass
a fixed width; tool chips hug and clamp with `overflow_hidden`. Chip id =
segment id for element identity and overlay targeting.

Icon/status table:

| Segment state       | Icon slot              | Tint            |
|---------------------|------------------------|-----------------|
| Thinking active     | spinner                | accent          |
| Thinking done       | Brain                  | muted           |
| Tool running        | spinner                | accent          |
| Tool completed      | Wrench                 | muted           |
| Tool failed         | TriangleAlert          | danger          |
| Tool cancelled      | CircleStop             | muted           |

## Detail overlay

New desktop layer kind `LayerKind::ChipDetail` plus shell state
`chip_detail: Option<ChipDetailTarget>` (`row id` + `segment id`). Opening
re-resolves the payload from the client-core projection at render time —
no cached copy to invalidate:

- `Thinking` → scrollable mono/meta text panel.
- `Tool` → existing memoized structured sections (`format_tool_body`),
  header shows name + status.

Presentation reuses island `render_overlay_layer_on` Dialog style with the
established backdrop/Escape contract (`TemporaryLayers`); focus restores to
the timeline on close. Because the layer renders above the timeline without
touching list geometry, no remeasure occurs.

Removed from F-45/D-62: thinking/tool independent cards, inline collapse
(`card_body_open` for those arms), `CollapsePolicy::StartCollapsed` there,
per-card streaming carets outside text tails. `block_expand` survives for
user overflow only.

## Package impact

| Package | Change |
|---|---|
| `island` | `components/activity_chip.rs`: `ActivityChip` + `ActivityStatus` (+ row helper); theme icons if missing |
| `piko-desktop` | `timeline.rs` row types + segment merge; `timeline/frame.rs` groups; `rows.rs` flow rendering + chip wiring; `focus.rs`/`layers.rs` chip-detail layer; `tool_body.rs` reused by overlay |
| docs | F-46 PRD; this design; index rows; F-45 amendment note |

## Risks

- Group mapping bugs would misplace tools across turns → parity tests over
  every adjacency (user/draft/tool/assistant/session-entry).
- Overlay re-resolution must tolerate a chip whose tool vanished between
  click and paint (agent switch) → render empty-safe.
- Fixed-width thinking chips in narrow windows → width clamps to column.
