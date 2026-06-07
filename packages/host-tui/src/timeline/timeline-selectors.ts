// ============================================================================
// Timeline selectors — derived data from timeline state
// ============================================================================

import type { TimelineItem, TuiTimelineState } from "./types.js";

/**
 * Get visible items from the timeline state.
 * Collapsed tool calls are still shown (just in collapsed form).
 */
export function selectTimelineItems(state: TuiTimelineState): TimelineItem[] {
  return state.items;
}

/**
 * Get the item count.
 */
export function selectTimelineItemCount(state: TuiTimelineState): number {
  return state.items.length;
}

/**
 * Check if there are pending new items (user scrolled away while streaming).
 */
export function selectPendingCount(state: TuiTimelineState): number {
  return state.pendingNewItems;
}

/**
 * Check if a tool call is expanded.
 */
export function isToolExpanded(state: TuiTimelineState, toolCallId: string): boolean {
  return state.expandedItemIds.has(toolCallId);
}

/**
 * Check if a tool call is collapsed.
 */
export function isToolCollapsed(state: TuiTimelineState, toolCallId: string): boolean {
  return state.collapsedToolCallIds.has(toolCallId);
}
