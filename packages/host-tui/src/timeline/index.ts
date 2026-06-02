// ============================================================================
// Timeline — public API
// ============================================================================

export { ScrollController } from "./scroll-controller.js";
export {
  buildTimelineItems,
  finalizeStreamingItem,
  updateStreamingItem,
} from "./timeline-builder.js";
export { type TimelineAction, timelineReducer } from "./timeline-reducer.js";
export {
  isToolCollapsed,
  isToolExpanded,
  selectPendingCount,
  selectTimelineItemCount,
  selectTimelineItems,
} from "./timeline-selectors.js";
export type {
  TimelineAnchor,
  TimelineItem,
  TimelineItemKind,
  TimelineLayout,
  TuiTimelineState,
} from "./types.js";
export { createDefaultTimelineState } from "./types.js";
