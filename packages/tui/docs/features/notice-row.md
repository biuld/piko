# Notice Row

> Status: reviewed

## Overview

The Notice Row is the notice projection shown in the resident
[Guidance Row](./guidance-row.md) above the Composer. It shows transient
feedback and unresolved warnings or errors without adding client-local records
to the durable session Timeline. Hostd remains authoritative for approvals,
interactions, sessions, and other user-visible product state; notices are a
presentation projection of that state.

## Layout

```text
Timeline
Guidance Row   ⓘ/▲/✗ concise message · F8 dismiss | contextual hint
Suggestions?
Composer
BottomBar
```

Guidance always occupies one line. A visible notice replaces its contextual
hint without changing geometry. Actionable notices take precedence over
transient informational notices. Both the row and history panel use a distinct
glyph and theme color for each severity instead of spelling out level names.

`/noti` opens a centered modal containing the in-memory notice queue. Its title
uses the shared mode-strip affix:

```text
┌─ Notifications                 [Current] | All ─┐
│ ▲ session:a1b2c3d4 · pending · active     [Copy] │
│   Approval requested for a command that is too   │
│   long to fit on one line                        │
│ ⓘ global · transient · elapsed            [Copy] │
│   Setting applied                                │
│                                              │
│ ↑/↓ select · c/Enter copy · PgUp/PgDn scroll │
└──────────────────────────────────────────────┘
```

Each notice is a two-row minimum unit. Its first row contains the severity
glyph, scope, policy, lifecycle status, and a right-aligned `[Copy]` action.
The second row begins the original message; longer messages wrap to aligned
continuation rows. Info, warning, and error use `ⓘ`, `▲`, and `✗` respectively;
level names are not rendered.

`Current` is selected whenever the modal opens and shows global notices plus
notices scoped to the currently viewed session. `All` includes notices scoped
to other sessions. Clicking the title affix or pressing Tab changes scope.

## Behavior / interactions

- Notices are append-only for the lifetime of the TUI process and have stable
  local ids. No severity or lifecycle is capacity-evicted.
- A notice is global, session-scoped, or agent-scoped. Only notices applicable
  to the viewed session/agent are eligible for display.
- Informational notices stop appearing in the Notice Row after a short display
  window, but remain in the in-memory queue and `/noti` for the lifetime of the
  TUI process.
- Dismissible notices remain in the row until dismissed. State-derived notices
  remain in the row until resolved by a stable subject such as an approval id.
- Dismiss and resolve update presentation state; they never delete the notice
  record. Older applicable active notices may then become visible in the row.
- `/noti` shows active, elapsed, dismissed, and resolved records.
- Applying an authoritative snapshot may recreate state-derived notices from
  pending host state. The notice queue itself is never persisted.
- `/noti` does not dismiss or resolve items. Up/Down selects a notice; `c`,
  Enter, or its `[Copy]` action copies the complete original message without
  severity or lifecycle metadata. After the platform clipboard confirms
  success, that row shows a success-colored `[Copied]` label for a short
  interval. PageUp/PageDown and the mouse wheel scroll the modal; Esc or outside
  click closes it.

## Configuration

| Binding id | Default | Behavior |
|---|---|---|
| `app.notifications.clear` | F8 | Dismiss the currently visible notice |

`/noti` is an always-available TUI-local presentation command.

The informational row-display duration is an implementation constant, not a
user setting. The entire queue is destroyed when the TUI process exits.

## Non-goals

- Desktop or operating-system notifications.
- A durable notification event log.
- Persisting dismissal state.
- Using notices as the source of truth for pending approvals or interactions.
- Rendering session summaries, branch metadata, or custom messages.
- Persisting modal scope or scroll position after the modal closes.
