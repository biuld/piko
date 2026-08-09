# Notice Row Design

## Selected Feature

This design implements [notice-row.md](../features/notice-row.md).

## Ownership

`AppState` owns a `NoticeCenter`. The center owns lifecycle and selection; the
Notice Row renderer paints the selected item, and pointer/key routing emits a
dismiss-visible action. Protocol and hostd do not store this queue.

```text
typed host event/snapshot ──→ AppState reducer ──→ NoticeCenter
local command/runtime error ┘                         │
                                                     ├─ visible_for(view scope)
Tick ──→ expire(now)                                 └─ dismiss / resolve
```

The same center backs a `SurfaceId::Notifications` centered modal. Modal-local
scope and scroll offset remain presentation state inside the center and reset
to Current/zero whenever `/noti` opens.

## Model

Each notice has a local id, severity, scope, lifecycle, message, and optional
stable subject. Severity controls presentation. Lifecycle independently
controls expiry and resolution.

- `Transient`: expires at a monotonic deadline.
- `Dismissible`: remains until the user dismisses it.
- `UntilResolved`: remains until its subject is resolved or the user dismisses
  it.

Transient and attention notices have separate capacity limits so routine info
feedback cannot evict unresolved warnings/errors. When any attention notice is
applicable it wins over newer transient notices.

## Modal Projection

The modal uses `PaneTitleAffix::ModeStrip(["Current", "All"])`. Current
projects `Global + Session(active_session_id)`; All projects every in-memory
item and prints each item's scope. Items are newest-first and use one display
row each so the stored scroll offset maps directly to rows. Pane title-affix
regions provide pointer switching; Tab provides the keyboard-equivalent action.

`/noti` is merged through the TUI-local command catalog and never appears in
hostd's neutral command catalog.

## Projection Rules

- Approval requested: session-scoped `UntilResolved(Approval(id))` warning.
- Approval resolved: resolve `Approval(id)`.
- Host transport/decode and command failures: global or session-scoped
  dismissible errors; they are not inserted into Timeline.
- Auth lifecycle: global notices; auth state is not an agent conversation item.
- Session snapshot: notices may be re-derived from pending authoritative state.

## Timeline Boundary

The durable session ledger retains typed `SessionTreeEntry` facts. Snapshot
projection maps them directly to typed Timeline components:

| Entry | Projection |
|---|---|
| Message | user/assistant/tool-result component |
| ToolCall | tool component |
| ModelChange / ThinkingLevelChange | session fact |
| ActiveToolsChange | session fact when non-empty |
| Compaction / BranchSummary | summary component |
| CustomMessage with `display = true` | custom-message component |
| Custom, Label, SessionInfo, Leaf | no Timeline component |

Timeline components derived from session entries use the durable entry id.
Snapshot rebuild and live replay must converge. A generic persisted Notice
entry is intentionally not introduced.

## Verification

- Transient notices expire without evicting attention notices.
- Resolving a subject removes its notice.
- Dismiss removes only the visible notice.
- Session-scoped notices do not leak across session views.
- Snapshot metadata does not produce empty or navigation-id notices.
- Summaries and visible custom messages retain dedicated component kinds.
