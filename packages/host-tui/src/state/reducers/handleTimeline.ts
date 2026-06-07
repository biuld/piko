// ============================================================================
// Timeline reducers — scroll, jump, expand, collapse, pending
// ============================================================================

import type {
  TimelineItemToggledEvent,
  TimelineJumpLatestEvent,
  TimelinePendingUpdateEvent,
  TimelineScrolledEvent,
  TimelineToolToggledEvent,
} from "../events.js";
import type { TuiState } from "../state.js";

export function handleTimelineScrolled(state: TuiState, event: TimelineScrolledEvent): TuiState {
  return {
    ...state,
    timeline: {
      ...state.timeline,
      anchor: event.anchor,
      atBottom: event.atBottom,
      userScrolled: event.anchor === "manual",
      pendingNewItems: event.anchor === "bottom" ? 0 : state.timeline.pendingNewItems,
    },
  };
}

export function handleTimelineJumpLatest(
  state: TuiState,
  _event: TimelineJumpLatestEvent,
): TuiState {
  return {
    ...state,
    timeline: {
      ...state.timeline,
      anchor: "bottom",
      atBottom: true,
      userScrolled: false,
      pendingNewItems: 0,
    },
  };
}

export function handleTimelineItemToggled(
  state: TuiState,
  event: TimelineItemToggledEvent,
): TuiState {
  const newExpanded = new Set(state.timeline.expandedItemIds);
  if (newExpanded.has(event.itemId)) {
    newExpanded.delete(event.itemId);
  } else {
    newExpanded.add(event.itemId);
  }
  return {
    ...state,
    timeline: { ...state.timeline, expandedItemIds: newExpanded },
  };
}

export function handleTimelineToolToggled(
  state: TuiState,
  event: TimelineToolToggledEvent,
): TuiState {
  const newCollapsed = new Set(state.timeline.collapsedToolCallIds);
  if (newCollapsed.has(event.toolCallId)) {
    newCollapsed.delete(event.toolCallId);
  } else {
    newCollapsed.add(event.toolCallId);
  }
  return {
    ...state,
    timeline: { ...state.timeline, collapsedToolCallIds: newCollapsed },
  };
}

export function handleTimelinePendingUpdate(
  state: TuiState,
  event: TimelinePendingUpdateEvent,
): TuiState {
  return {
    ...state,
    timeline: { ...state.timeline, pendingNewItems: event.pendingNewItems },
  };
}

export function handleTimelineToggleAllTools(state: TuiState): TuiState {
  const current = state.timeline.collapsedToolCallIds;
  // If any are collapsed, expand all (clear set).
  // If all are expanded, collapse all tool results.
  if (current.size > 0) {
    return {
      ...state,
      timeline: { ...state.timeline, collapsedToolCallIds: new Set() },
    };
  }
  // Collapse all tool-result items
  const allToolIds = new Set<string>();
  for (const item of state.timeline.items) {
    if (item.toolCallId) allToolIds.add(item.toolCallId);
  }
  return {
    ...state,
    timeline: { ...state.timeline, collapsedToolCallIds: allToolIds },
  };
}
