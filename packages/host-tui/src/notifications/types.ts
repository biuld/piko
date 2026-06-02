// ============================================================================
// Notification types — TUI notification model
// ============================================================================

export type NotificationSeverity = "info" | "success" | "warning" | "error";

export type NotificationSource =
  | "runtime"
  | "engine"
  | "tool"
  | "command"
  | "auth"
  | "model"
  | "session"
  | "extension"
  | "ui";

export interface TuiNotification {
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

export interface TuiNotificationState {
  items: TuiNotification[];
  maxItems: number;
  latestId?: string;
  unreadCount: number;
}

export interface NotifyInput {
  severity?: NotificationSeverity;
  source?: NotificationSource;
  message: string;
  detail?: string;
  ttlMs?: number;
  commandId?: string;
  toolCallId?: string;
  dedupeKey?: string;
}

export type NotificationEvent =
  | { type: "notification_added"; notification: TuiNotification }
  | { type: "notification_cleared"; id?: string }
  | { type: "notification_read"; id?: string };

export interface NotificationFilter {
  severity?: NotificationSeverity;
  source?: NotificationSource;
  unreadOnly?: boolean;
}
