# Notification Center

## Overview

Notification Center keeps a temporary, window-local history of the feedback
that piko has shown as toast notifications. A bell in the window title bar
opens the history without replacing the current Workbench or Settings surface.

## Layout

```text
TitleBar                                      [Bell] [Settings]
                                                  ┌─────────────┐
                                                  │ Notifications│
                                                  │ newest first │
                                                  │ ...          │
                                                  └─────────────┘
```

- The bell is always visible immediately to the left of Settings in both the
  Workbench and Settings title bars.
- The notification panel floats below the trailing title-bar actions and does
  not dim or replace the active archipelago.
- Toasts appear at a stable upper-right position below the title bar while the
  notification panel is closed.

## Behavior / interactions

- Clicking the bell toggles the notification panel.
- Clicking outside the panel, clicking the bell again, or pressing `Escape`
  closes it.
- Opening the panel marks every current notification as read. New notifications
  arriving while it is open are shown immediately and are considered read.
- Opening the panel dismisses visible toasts. While the panel remains open, new
  notifications enter its history directly without displaying a duplicate
  toast.
- An unread dot on the bell indicates that a notification has appeared since the
  notification history was last viewed.
- Every notification is added to history before an eligible toast is shown.
  Auto-hiding or manually dismissing the toast does not remove its history
  entry.
- History is newest first and shows severity, title, message, and relative time.
- A notification can be removed individually. Clear All removes the complete
  history and clears visible toasts.
- The history is bounded to the 100 most recent notifications. When empty, the
  panel presents a quiet empty state.
- Notification Center records toast feedback only. Running Agents and tools,
  pending prompts, queues, and other live state remain in Activity Center.
- Informational, success, warning, and error feedback share the same severity
  vocabulary in toasts and history.

## Configuration

There are no new settings or keyboard shortcuts. Notification history, unread
state, and panel visibility are window-local presentation state.

## Non-goals

- Persistence across window or application restarts.
- Operating-system notifications.
- Notification preferences or per-category filtering.
- Duplicating Activity Center or Timeline content.
- Navigation from a notification to a Session, Agent, or transcript location.
- Cross-window notification synchronization.
