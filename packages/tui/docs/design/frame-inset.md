# Chat horizontal inset — design

Feature Doc: `docs/features/frame-inset.md`

## Goal

Keep unframed chat surfaces (Timeline, AgentPanel) away from the left/right
terminal edges without floating bordered overlays inside a global gutter.

## Responsibilities

| Layer | Responsibility |
|-------|----------------|
| `layout::inset_horizontal` | Pure geometry: shrink a `Rect` on left/right only |
| `Timeline::render` | `[left inset][content][scrollbar]`; scrollbar is the right gutter |
| `AgentPanelState::render` | Full-width top border; inset only the content rows |
| `render::render` | Split the full `frame.area()`; no outer inset |
| Other panels | Remain edge-flush; own any internal padding |

## Approach

1. Slot layout uses the full terminal frame.
2. Timeline uses left inset only; the scrollbar column is the right gutter.
3. AgentPanel keeps a full-width top border and insets only the content rows.
4. No settings schema in v1.

## Clamp policy

Horizontal inset is limited to `(width.saturating_sub(1) / 2)` so a non-empty
area always retains at least one content column.

## Tradeoffs

- Sharing one constant keeps Timeline and AgentPanel columns aligned.
- Editor/BottomBar stay flush by design; if those later need padding, handle it
  inside those panels rather than restoring a global frame inset.
