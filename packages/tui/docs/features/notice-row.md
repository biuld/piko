# Notice Row

> Status: reviewed

## Overview

The Notice Row is the TUI's in-memory attention surface above the Composer. It
shows transient feedback and unresolved warnings or errors without adding
client-local records to the durable session Timeline. Hostd remains
authoritative for approvals, interactions, sessions, and other user-visible
product state; notices are a presentation projection of that state.

## Layout

```text
Timeline
Notice Row?    ● warning/error/info · concise message · F8 dismiss
Suggestions?
Composer
BottomBar
```

The row occupies one line only while a notice is visible. Actionable notices
take precedence over transient informational notices. Severity is expressed by
both a glyph/word and a theme color.

`/noti` opens a centered read-only modal containing the in-memory notice
queue. Its title uses the shared mode-strip affix:

```text
┌─ Notifications                 [Current] | All ─┐
│ ● warning  current session  approval requested  │
│ ● info     global           setting applied     │
│                                              │
│ Tab scope · ↑/↓ scroll · Esc close           │
└──────────────────────────────────────────────┘
```

`Current` is selected whenever the modal opens and shows global notices plus
notices scoped to the currently viewed session. `All` includes notices scoped
to other sessions. Clicking the title affix or pressing Tab changes scope.

## Behavior / interactions

- Notices are kept only in TUI memory and have stable local ids.
- A notice is global, session-scoped, or agent-scoped. Only notices applicable
  to the viewed session/agent are eligible for display.
- Informational notices expire automatically and never evict unresolved
  warning/error notices.
- Dismissible notices remain until dismissed. State-derived notices may also
  be resolved by a stable subject such as an approval id.
- Resolving an approval or interaction removes its associated notice.
- Clicking the row or pressing the dismiss binding removes only the currently
  visible notice; older applicable notices may then become visible.
- Applying an authoritative snapshot may recreate state-derived notices from
  pending host state. The notice queue itself is never persisted.
- `/noti` does not dismiss or resolve items. Up/Down, PageUp/PageDown, and the
  mouse wheel scroll the modal; Esc or outside click closes it.

## Configuration

| Binding id | Default | Behavior |
|---|---|---|
| `app.notifications.clear` | F8 | Dismiss the currently visible notice |

`/noti` is an always-available TUI-local presentation command.

The informational expiry duration and queue limits are implementation
constants, not user settings.

## Non-goals

- Desktop or operating-system notifications.
- A durable notification event log.
- Persisting dismissal state.
- Using notices as the source of truth for pending approvals or interactions.
- Rendering session summaries, branch metadata, or custom messages.
- Persisting modal scope or scroll position after the modal closes.
