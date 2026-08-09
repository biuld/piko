# Notice Row Design

## Selected Feature

This design implements [notice-row.md](../features/notice-row.md).

## Ownership

`AppState` owns a `NoticeCenter`. The center owns append-only records,
presentation state, and selection; the Notice Row renderer paints the selected
item, and pointer/key routing emits a dismiss-visible action. Protocol and
hostd do not store this queue.

```text
typed host event/snapshot ──→ AppState reducer ──→ NoticeCenter
local command/runtime error ┘                         │
                                                     ├─ row_visible_for(now, scope)
Tick ──→ row projection clock                        └─ mark dismissed / resolved
```

The same center backs a `SurfaceId::Notifications` centered modal. Modal-local
scope and scroll offset remain presentation state inside the center and reset
to Current/zero whenever `/noti` opens.

## Model

Each notice has a local id, severity, scope, policy, status, message, and
optional stable subject. Severity controls presentation. Policy controls when
an active record is eligible for the Notice Row. Status is `Active`,
`Dismissed`, or `Resolved`.

- `Transient`: remains in memory, but is eligible for Notice Row projection
  only until a monotonic deadline.
- `Dismissible`: remains in the row until the user dismisses it.
- `UntilResolved`: remains in the row until its subject is resolved or the
  user dismisses it.

Records are never removed or capacity-evicted. Dismiss, resolution, and an
elapsed transient deadline only change row eligibility or status. When any
active attention notice is applicable it wins over newer transient notices.

Snapshot reconciliation marks active state-derived records resolved, then
reactivates matching authoritative pending subjects in place. This preserves
append-only history without duplicating the same pending approval or
interaction every time a snapshot is applied.

## Modal Projection

The modal uses `PaneTitleAffix::ModeStrip(["Current", "All"])`. Current
projects `Global + Session(active_session_id)`; All projects every in-memory
item and prints each item's scope, policy, and current status (`active`,
`elapsed`, `dismissed`, or `resolved`). Items are newest-first and use one
display row each so the stored scroll offset maps directly to rows. Pane
title-affix regions provide pointer switching; Tab provides the
keyboard-equivalent action.

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

- Elapsed transient notices leave the Notice Row but remain in `/noti` memory.
- Resolving a subject marks its notice resolved without deleting it.
- Dismiss marks only the visible notice dismissed without deleting it.
- More than 32 attention notices remain available in append-only history.
- Session-scoped notices do not leak across session views.
- Snapshot metadata does not produce empty or navigation-id notices.
- Summaries and visible custom messages retain dedicated component kinds.
