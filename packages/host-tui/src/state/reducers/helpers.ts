// ============================================================================
// Reducer helpers — shared utilities used across event handlers
// ============================================================================

import type { TimelineAnchor, TimelineItem } from "../../timeline/types.js";
import type { TuiMessageViewModel } from "../state.js";

let messageIdSeq = 0;
/** Session-scoped timestamp prefix to avoid collisions across restarts/resumes. */
const sessionTs = Date.now().toString(36);

export function nextMessageId(): string {
  return `msg-${sessionTs}-${++messageIdSeq}`;
}

/**
 * Seed the message ID sequence from an existing transcript (e.g. on session resume).
 * Parses IDs like msg-<ts>-<seq> and sets seq to max+1.
 */
export function seedMessageIdSeq(existingIds: string[]): void {
  let max = 0;
  for (const id of existingIds) {
    const parts = id.split("-");
    if (parts.length >= 3 && parts[0] === "msg") {
      const seq = parseInt(parts[parts.length - 1], 10);
      if (!Number.isNaN(seq) && seq > max) max = seq;
    }
  }
  if (max >= messageIdSeq) messageIdSeq = max;
}

export function findLastAssistantIndex(transcript: TuiMessageViewModel[]): number {
  for (let i = transcript.length - 1; i >= 0; i--) {
    if (transcript[i].role === "assistant") return i;
  }
  return -1;
}

export function findToolCallIndex(transcript: TuiMessageViewModel[], toolCallId: string): number {
  for (let i = transcript.length - 1; i >= 0; i--) {
    const msg = transcript[i];
    if (msg.toolBlock?.toolCallId === toolCallId) return i;
  }
  return -1;
}

export function findToolEntityIndex(
  transcript: TuiMessageViewModel[],
  toolEntityId: string,
): number {
  for (let i = transcript.length - 1; i >= 0; i--) {
    if (transcript[i].toolBlock?.toolEntityId === toolEntityId) return i;
  }
  return -1;
}

/**
 * Append a timeline item, respecting the user-scrolled-away state.
 */
export function pushTimelineItem(
  items: TimelineItem[],
  item: TimelineItem,
  anchor: TimelineAnchor,
): { items: TimelineItem[]; pendingDelta: number } {
  return {
    items: [...items, item],
    pendingDelta: anchor === "manual" ? 1 : 0,
  };
}
