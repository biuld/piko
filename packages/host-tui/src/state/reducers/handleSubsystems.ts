// ============================================================================
// Subsystem reducers — usage, extensions, notifications, surfaces, focus
// ============================================================================

import type {
  FocusChangedEvent,
  NotificationAddedEvent,
  NotificationClearedEvent,
  NotificationReadEvent,
  SurfaceClosedEvent,
  SurfaceOpenedEvent,
  UsageUpdatedEvent,
} from "../events.js";
import type { TuiState } from "../state.js";

export function handleUsageUpdated(state: TuiState, event: UsageUpdatedEvent): TuiState {
  return {
    ...state,
    usage: {
      inputTokens: event.inputTokens ?? state.usage.inputTokens,
      outputTokens: event.outputTokens ?? state.usage.outputTokens,
      cacheReadTokens: event.cacheReadTokens ?? state.usage.cacheReadTokens,
      cacheWriteTokens: event.cacheWriteTokens ?? state.usage.cacheWriteTokens,
      totalCost: event.totalCost ?? state.usage.totalCost,
      contextWindow: event.contextWindow ?? state.usage.contextWindow,
      contextPercent: event.contextPercent ?? state.usage.contextPercent,
    },
  };
}

export function handleNotificationAdded(state: TuiState, event: NotificationAddedEvent): TuiState {
  const notifs = [event.notification, ...state.notifications].slice(0, 200);
  return { ...state, notifications: notifs };
}

export function handleNotificationCleared(
  state: TuiState,
  event: NotificationClearedEvent,
): TuiState {
  if (event.id) {
    return {
      ...state,
      notifications: state.notifications.filter((n) => n.id !== event.id),
    };
  }
  return { ...state, notifications: [] };
}

export function handleNotificationRead(state: TuiState, event: NotificationReadEvent): TuiState {
  const updatedNotifs = state.notifications.map((n) => {
    if (!event.id || n.id === event.id) {
      return { ...n, readAt: n.readAt ?? Date.now() };
    }
    return n;
  });
  return { ...state, notifications: updatedNotifs };
}

export function handleSurfaceOpened(state: TuiState, event: SurfaceOpenedEvent): TuiState {
  return { ...state, surfaces: [...state.surfaces, event.surface] };
}

export function handleSurfaceClosed(state: TuiState, event: SurfaceClosedEvent): TuiState {
  const closedIds = new Set<string>([event.surfaceId]);
  for (const s of state.surfaces) {
    if (s.parentId && closedIds.has(s.parentId)) {
      closedIds.add(s.id);
    }
  }
  const remaining = state.surfaces.filter((s) => !closedIds.has(s.id));
  return {
    ...state,
    surfaces: remaining,
    layout: {
      ...state.layout,
      activeRegion: remaining.length === 0 ? "editor" : state.layout.activeRegion,
    },
  };
}

export function handleFocusChanged(state: TuiState, event: FocusChangedEvent): TuiState {
  return {
    ...state,
    focus: {
      ...state.focus,
      activeOwnerId: event.activeOwnerId,
      region: event.region,
    },
  };
}
