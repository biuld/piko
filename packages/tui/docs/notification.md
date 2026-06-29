# Notification

## Overview

Notification is the TUI's user-facing alert system. It surfaces warnings, errors,
and transient status updates without disrupting the main workflow. Notifications
are non-blocking — they never steal focus or require interaction to continue.

## Levels

| Level     | Meaning                                      | Persistence       | Display                 |
|-----------|----------------------------------------------|-------------------|-------------------------|
| `Info`    | Transient status (e.g. "session created")    | Auto-dismiss      | Status area only        |
| `Warning` | Actionable alert (e.g. "approval requested") | Until dismissed   | NotificationRow         |
| `Error`   | Failure that needs attention                 | Until dismissed   | NotificationRow (emph.) |

Info messages are transient — they appear briefly in the status area and
disappear without user action. They do **not** occupy the NotificationRow.

Warnings and errors persist until the user clears them (`Ctrl-L`). They occupy
the NotificationRow so they remain visible even while a turn is running.

## UI component: NotificationRow

A single inline row between the AgentPanel and the editor. It shows the most
recent actionable (warning or error) notification.

```
 │   approval requested for bash
```

- **Marker**: `│` (U+2502 box drawings light vertical)
- **Color**: yellow for warning, red for error
- **Content**: the notification message, truncated to fit terminal width
- **Visibility**: shown whenever at least one warning or error exists,
  regardless of whether a turn is running. Hidden when all actionable
  notifications have been dismissed.

When multiple actionable notifications are pending, only the most recent one is
shown. The user dismisses them one by one with `Ctrl-L`, or all at once
(behavior TBD).

## Lifecycle

```
push(level, message)
  │
  ├─ level == Info ──► status area briefly ──► auto-dismiss
  │
  └─ level == Warning | Error
        │
        ├─ NotificationRow appears (if hidden)
        ├─ persists across turns
        └─ dismiss ──► removed
              │
              └─ if no more actionable ──► NotificationRow hides
```

## Configuration

| Key                                  | Type   | Default | Description                              |
|--------------------------------------|--------|---------|------------------------------------------|
| `tui.notifications.maxVisible`       | `u8`   | `1`     | Max notification rows to show at once    |
| `tui.notifications.infoDurationMs`   | `u64`  | `3000`  | How long info messages stay visible (ms) |

## Key bindings

| Key     | Action                          |
|---------|---------------------------------|
| Ctrl-L  | Dismiss the current notification |

## Non-goals

- Notification history panel (future, but not now)
- Notification grouping / stacking (multiple rows is future work)
- Desktop notifications (osascript / notify-send)
- Persisting notifications across TUI restarts
