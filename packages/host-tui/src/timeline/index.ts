// ============================================================================
// Timeline — public API
// ============================================================================

export type {
  ProjectionDiagnostic,
  TimelineProjection,
} from "./projection.js";
export {
  buildOrderedProjection,
  createProjection,
  upsertAssistantMessage,
  upsertToolItem,
  upsertUserMessage,
  validateAndApplySeq,
} from "./projection.js";
export { ScrollController } from "./scroll-controller.js";
export {
  buildTimelineItem,
  createStreamingTimelineItem,
  finalizeStreamingTimelineItem,
  initTimelineItems,
  updateStreamingTimelineItem,
} from "./timeline-builder.js";
export {
  isToolCollapsed,
  isToolExpanded,
  selectPendingCount,
  selectTimelineItemCount,
  selectTimelineItems,
} from "./timeline-selectors.js";
export type {
  FinalizeOptions,
  FinalizeResult,
  ReconcileOptions,
  ReconcileResult,
} from "./transcript-reconcile.js";
export {
  finalizeProjection,
  reconcileLegacyTranscript,
  validateCommittedTranscript,
} from "./transcript-reconcile.js";
export type {
  TimelineAnchor,
  TimelineItem,
  TimelineItemKind,
  TimelineLayout,
  TuiTimelineState,
} from "./types.js";
export { createDefaultTimelineState } from "./types.js";
