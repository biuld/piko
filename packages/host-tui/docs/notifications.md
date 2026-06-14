# Notification System

Notifications are a first-class in-memory subsystem for the current TUI session.

The status line currently composes strings from stream state, queue info, and extension slots. That is useful for progress, but not enough for runtime notices because there is no history, source, severity, timestamp, read state, TTL, dedupe, or slash command to inspect previous notices.

## Goals

- Collect notifications from runtime, commands, tools, extensions, auth, model switching, and UI subsystems.
- Show the latest relevant notification in the status line in real time.
- Keep an in-memory list for the current session.
- Provide `/notifications` and `/noti`.
- Keep notification state separate from bottom bar and transcript.

## Model

```ts
type NotificationSeverity = "info" | "success" | "warning" | "error";

type NotificationSource =
  | "runtime"
  | "engine"
  | "tool"
  | "command"
  | "auth"
  | "model"
  | "session"
  | "extension"
  | "ui";

interface TuiNotification {
  id: string;
  severity: NotificationSeverity;
  source: NotificationSource;
  message: string;
  detail?: string;
  createdAt: number;
  readAt?: number;
  ttlMs?: number;
  sessionId?: string;
  commandId?: string;
  toolCallId?: string;
  dedupeKey?: string;
}

interface TuiNotificationState {
  items: TuiNotification[];
  maxItems: number;
  latestId?: string;
  unreadCount: number;
}
```

Default policy:

- Keep latest 200 notifications in memory.
- Deduplicate repeated notices by `dedupeKey`.
- Expire status-line visibility through `ttlMs`, but keep history until it falls out of `maxItems`.
- Do not persist notifications to disk in the first pass.
- Do not add notifications to transcript unless they are user-visible agent messages.

## NotificationCenter

```ts
interface NotificationCenter {
  notify(input: NotifyInput): string;
  clear(id?: string): void;
  markRead(id?: string): void;
  latest(): TuiNotification | undefined;
  list(filter?: NotificationFilter): TuiNotification[];
}
```

Events:

```ts
type NotificationEvent =
  | { type: "notification_added"; notification: TuiNotification }
  | { type: "notification_cleared"; id?: string }
  | { type: "notification_read"; id?: string };
```

`NotificationCenter.notify()` dispatches state events; it does not mutate component-local state.

## Status line integration

Priority order:

1. active stream/tool progress
2. latest unexpired notification
3. queue info
4. extension status slots

The status line displays only the latest notification by default. Full history belongs in notification history.

Severity styling:

- `info`: muted/accent
- `success`: success token
- `warning`: warning token
- `error`: error token

## Slash commands

- `/notifications`
- `/noti`

Behavior:

- Opens notification history for current session.
- Newest first.
- Later: filter by severity/source.
- `Enter` expands a notification with detail.
- `Esc` returns focus to editor or parent surface.
- `c` can clear notifications after keymap support exists.

Surface:

- `side-drawer` on wide terminals.
- `replace-slot(timeline)` on narrow terminals or very long histories.
- Never use a blocking form for notification history.

## Producers

Initial producers:

- unknown slash command
- command unavailable because agent is running
- model switch success/failure
- login/auth success/failure
- tool approval needed/denied
- tool execution error
- runtime shutdown/restore errors
- session resume/import/export success/failure
- settings/theme load errors
- extension status/error messages

Do not use notifications for normal token usage, model name, cwd, or other stable status data. Those belong in bottom bar.
