// ============================================================================
// NotificationCenter — in-memory notification history for current session
// ============================================================================

import type {
  NotificationEvent,
  NotificationFilter,
  NotificationSource,
  NotifyInput,
  TuiNotification,
  TuiNotificationState,
} from "./types.js";

let notificationCounter = 0;

function nextId(): string {
  return `notif-${++notificationCounter}`;
}

const DEFAULT_MAX_ITEMS = 200;
const DEFAULT_TTL_MS = 10_000; // 10 seconds for status line visibility

export class NotificationCenter {
  private state: TuiNotificationState;
  private listeners: Array<(event: NotificationEvent) => void> = [];

  constructor(maxItems = DEFAULT_MAX_ITEMS) {
    this.state = {
      items: [],
      maxItems,
      unreadCount: 0,
    };
  }

  /**
   * Subscribe to notification events.
   */
  onEvent(fn: (event: NotificationEvent) => void): () => void {
    this.listeners.push(fn);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== fn);
    };
  }

  /**
   * Emit a notification event to all listeners.
   */
  private emit(event: NotificationEvent): void {
    for (const listener of this.listeners) {
      listener(event);
    }
  }

  /**
   * Add a notification. Returns the notification ID.
   */
  notify(input: NotifyInput): string {
    const {
      message,
      severity = "info",
      source = "ui",
      detail,
      ttlMs,
      commandId,
      toolCallId,
      dedupeKey,
    } = input;

    // Deduplicate by dedupeKey
    if (dedupeKey) {
      const existing = this.state.items.find((n) => n.dedupeKey === dedupeKey);
      if (existing) {
        // Update existing notification
        existing.message = message;
        existing.severity = severity;
        existing.detail = detail;
        existing.readAt = undefined;
        this.state.latestId = existing.id;
        this.emit({ type: "notification_added", notification: existing });
        return existing.id;
      }
    }

    const id = nextId();
    const notification: TuiNotification = {
      id,
      severity,
      source: source as NotificationSource,
      message,
      detail,
      createdAt: Date.now(),
      ttlMs: ttlMs ?? DEFAULT_TTL_MS,
      commandId,
      toolCallId,
      dedupeKey,
    };

    this.state.items.unshift(notification);
    this.state.latestId = id;
    this.state.unreadCount++;

    // Prune old items
    if (this.state.items.length > this.state.maxItems) {
      this.state.items = this.state.items.slice(0, this.state.maxItems);
    }

    this.emit({ type: "notification_added", notification });
    return id;
  }

  /**
   * Clear a specific notification by ID, or all notifications.
   */
  clear(id?: string): void {
    if (id) {
      const before = this.state.items.length;
      this.state.items = this.state.items.filter((n) => n.id !== id);
      if (this.state.items.length < before) {
        this.state.unreadCount = Math.max(
          0,
          this.state.unreadCount - (before - this.state.items.length),
        );
      }
      if (this.state.latestId === id) {
        this.state.latestId = this.state.items[0]?.id;
      }
    } else {
      this.state.items = [];
      this.state.latestId = undefined;
      this.state.unreadCount = 0;
    }

    this.emit({ type: "notification_cleared", id });
  }

  /**
   * Mark a notification as read.
   */
  markRead(id?: string): void {
    if (id) {
      const notification = this.state.items.find((n) => n.id === id);
      if (notification && !notification.readAt) {
        notification.readAt = Date.now();
        this.state.unreadCount = Math.max(0, this.state.unreadCount - 1);
        this.emit({ type: "notification_read", id });
      }
    } else {
      // Mark all as read
      for (const n of this.state.items) {
        if (!n.readAt) {
          n.readAt = Date.now();
        }
      }
      this.state.unreadCount = 0;
      this.emit({ type: "notification_read" });
    }
  }

  /**
   * Get the latest notification.
   */
  latest(): TuiNotification | undefined {
    if (!this.state.latestId) return undefined;
    return this.state.items.find((n) => n.id === this.state.latestId);
  }

  /**
   * Get the latest unexpired notification for status line display.
   */
  latestUnexpired(): TuiNotification | undefined {
    const now = Date.now();
    for (const n of this.state.items) {
      if (n.ttlMs && now - n.createdAt > n.ttlMs) continue;
      if (!n.readAt) return n;
    }
    return undefined;
  }

  /**
   * List notifications with optional filtering.
   */
  list(filter?: NotificationFilter): TuiNotification[] {
    let items = [...this.state.items];

    if (filter?.severity) {
      items = items.filter((n) => n.severity === filter.severity);
    }
    if (filter?.source) {
      items = items.filter((n) => n.source === filter.source);
    }
    if (filter?.unreadOnly) {
      items = items.filter((n) => !n.readAt);
    }

    return items;
  }

  /**
   * Get the current notification state snapshot.
   */
  getState(): TuiNotificationState {
    return { ...this.state, items: [...this.state.items] };
  }
}
