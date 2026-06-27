// ============================================================================
// Status data contract
//
// Status is a dedicated view between the timeline and editor. The data contract
// lives in state so reducers/selectors can build it without importing renderer
// components or renderer-only modules.
// ============================================================================

export type StatusState = "idle" | "working" | "compacting";

export interface QueueMessage {
  /** Truncated preview for display (one line) */
  preview: string;
  /** Full message content (for editor backfill) */
  content: string;
}

export interface StatusQueueContract {
  /** Steering messages queued to redirect the current turn */
  steering: QueueMessage[];
  /** Follow-up messages to run after current turn completes */
  followUp: QueueMessage[];
  /** Number of next-turn messages for the next user-initiated run */
  nextTurnCount: number;
}

export interface StatusNotification {
  severity: "info" | "success" | "warning" | "error";
  message: string;
}

export interface StatusContract {
  state: StatusState;
  /** Label override for the "working" state (defaults to "Working...") */
  label?: string;
  /** Queued messages (only relevant when state is "idle") */
  queue?: StatusQueueContract;
  /** Latest unexpired notification */
  notification?: StatusNotification;
}
