// ============================================================================
// Timeline reducers — jump latest, toggle all tools
// ============================================================================

import type { TimelineJumpLatestEvent } from "../events.js";
import type { TuiState } from "../state.js";

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
