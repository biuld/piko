// ============================================================================
// Notification selectors — derived data from notification state
// ============================================================================

import type { TuiNotification } from "./types.js";

/**
 * Check if a notification is expired (beyond its TTL for status line display).
 */
export function isNotificationExpired(notification: TuiNotification, now = Date.now()): boolean {
  if (!notification.ttlMs) return false;
  return now - notification.createdAt > notification.ttlMs;
}

/**
 * Get the severity color token for a notification.
 */
export function getSeverityColor(severity: string): string {
  switch (severity) {
    case "info":
      return "text.accent";
    case "success":
      return "ok";
    case "warning":
      return "text.warning";
    case "error":
      return "text.error";
    default:
      return "text.muted";
  }
}

/**
 * Get a severity icon for a notification.
 */
export function getSeverityIcon(severity: string): string {
  switch (severity) {
    case "info":
      return "ℹ";
    case "success":
      return "✓";
    case "warning":
      return "⚠";
    case "error":
      return "✗";
    default:
      return "•";
  }
}

/**
 * Format a timestamp for notification display.
 */
export function formatNotificationTime(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHour = Math.floor(diffMin / 60);

  if (diffSec < 60) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  if (diffHour < 24) return `${diffHour}h ago`;
  return date.toLocaleDateString();
}
