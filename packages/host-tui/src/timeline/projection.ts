// ============================================================================
// TimelineProjection — deterministic, ID-keyed timeline state.
//
// Items are ordered by session-transcript position, not event arrival time.
// Messages: inserted at end of orderedIds (append-only during live streaming).
// Tools:     inserted after their parent assistant message, ordered by
//            toolCallIndex within that parent.
//
// For legacy sessions, the original transcript order is preserved:
// tools remain adjacent to their parent assistant messages.
// ============================================================================

import type { TimelineItem } from "./types.js";

// ---- Projection state -------

export interface TimelineProjection {
  /** Ordered list of item IDs (msg:<id> or tool:<callId>). */
  orderedIds: string[];
  /** All items, keyed by stable projection ID. */
  itemsById: Record<string, TimelineItem>;
  /** Last applied eventSeq per runId for sequence validation. */
  lastAppliedSeqByRun: Record<string, number>;
  /** Tool items whose parent message hasn't arrived yet, keyed by parent message ID. */
  pendingTools: Record<string, TimelineItem[]>;
}

export function createProjection(): TimelineProjection {
  return {
    orderedIds: [],
    itemsById: {},
    lastAppliedSeqByRun: {},
    pendingTools: {},
  };
}

// ---- Pure reducer functions -------

/**
 * Upsert a user message into the projection. User messages always go at the end.
 */
export function upsertUserMessage(
  proj: TimelineProjection,
  item: TimelineItem,
): TimelineProjection {
  return upsertAtEnd(proj, item);
}

/**
 * Upsert an assistant message lifecycle event (start → update → end).
 * First occurrence inserts at end; updates keep position.
 * After inserting, re-parent any tools that arrived before their parent.
 */
export function upsertAssistantMessage(
  proj: TimelineProjection,
  item: TimelineItem,
): TimelineProjection {
  const messageId = item.messageId ?? (item.id.startsWith("msg:") ? item.id.slice(4) : item.id);
  const result = upsertAtEnd(proj, item);

  // Re-parent any pending tools that were waiting for this parent
  return reparentPendingTools(result, messageId);
}

/**
 * Upsert a tool execution item.
 * Positioned immediately after its parent assistant message in orderedIds.
 * If parent hasn't arrived yet, append at end (will be re-parented when parent arrives).
 */
export function upsertToolItem(
  proj: TimelineProjection,
  item: TimelineItem,
  parentMessageId: string,
  toolCallIndex: number,
): TimelineProjection {
  const exists = item.id in proj.itemsById;
  const itemsById = { ...proj.itemsById, [item.id]: item };

  if (exists) {
    // Update only, keep position
    return { ...proj, itemsById };
  }

  // Find parent assistant position
  const parentId = `msg:${parentMessageId}`;
  const parentIdx = proj.orderedIds.indexOf(parentId);

  if (parentIdx < 0) {
    // Parent not yet in projection — store in pending map
    const pending = { ...proj.pendingTools };
    const list = [...(pending[parentMessageId] ?? [])];
    list.push(item);
    // Keep sorted by toolCallIndex
    list.sort((a, b) => (a.toolCallIndex ?? 0) - (b.toolCallIndex ?? 0));
    pending[parentMessageId] = list;
    return {
      ...proj,
      itemsById,
      pendingTools: pending,
    };
  }

  // Find insertion point: after parent, before next message, by toolCallIndex
  const insertionIdx = findToolInsertionIndex(proj.orderedIds, parentIdx, toolCallIndex, itemsById);
  const orderedIds = [...proj.orderedIds];
  orderedIds.splice(insertionIdx, 0, item.id);

  return { ...proj, orderedIds, itemsById };
}

/** Upsert at end (for messages). */
function upsertAtEnd(proj: TimelineProjection, item: TimelineItem): TimelineProjection {
  const exists = item.id in proj.itemsById;
  const itemsById = { ...proj.itemsById, [item.id]: item };

  if (exists) {
    return { ...proj, itemsById };
  }

  return {
    ...proj,
    orderedIds: [...proj.orderedIds, item.id],
    itemsById,
  };
}

/**
 * Re-parent tools that are waiting for their parent message.
 * Called when a new assistant message is inserted.
 */
function reparentPendingTools(
  proj: TimelineProjection,
  parentMessageId: string,
): TimelineProjection {
  const pending = proj.pendingTools[parentMessageId];
  if (!pending || pending.length === 0) return proj;

  const parentId = `msg:${parentMessageId}`;
  const parentIdx = proj.orderedIds.indexOf(parentId);
  if (parentIdx < 0) return proj;

  // Remove from pending
  const newPending = { ...proj.pendingTools };
  delete newPending[parentMessageId];

  // Insert tools after parent in toolCallIndex order
  let orderedIds = [...proj.orderedIds];
  const insertPos = parentIdx + 1;
  const toolIds = pending.map((t) => t.id);
  orderedIds = [...orderedIds.slice(0, insertPos), ...toolIds, ...orderedIds.slice(insertPos)];

  return { ...proj, orderedIds, pendingTools: newPending };
}

/**
 * Validate and apply sequence monotonicity. Returns diagnostics for regressions.
 */
export function validateAndApplySeq(
  proj: TimelineProjection,
  runId: string,
  eventSeq: number,
): { proj: TimelineProjection; diagnostics: ProjectionDiagnostic[] } {
  const diagnostics: ProjectionDiagnostic[] = [];
  const prevSeq = proj.lastAppliedSeqByRun[runId] ?? -1;

  if (eventSeq <= prevSeq && prevSeq >= 0) {
    diagnostics.push({
      kind: "sequence_regression",
      runId,
      eventSeq,
      prevSeq,
    });
  }

  const lastAppliedSeqByRun = {
    ...proj.lastAppliedSeqByRun,
    [runId]: Math.max(prevSeq, eventSeq),
  };

  return { proj: { ...proj, lastAppliedSeqByRun }, diagnostics };
}

// ---- Diagnostics -------

export type ProjectionDiagnostic =
  | { kind: "sequence_regression"; runId: string; eventSeq: number; prevSeq: number }
  | { kind: "update_without_start"; itemId: string }
  | { kind: "duplicate_insert"; itemId: string }
  | { kind: "missing_parent"; toolId: string; parentMessageId: string }
  | { kind: "commit_order_mismatch"; expectedId: string; actualId: string };

// ---- Internal helpers -------

/** Find insertion index for a tool: after parent, before next message, ordered by toolCallIndex among siblings. */
export function findToolInsertionIndex(
  orderedIds: string[],
  parentIdx: number,
  toolCallIndex: number,
  itemsById: Record<string, TimelineItem>,
): number {
  // Start searching after the parent
  for (let i = parentIdx + 1; i < orderedIds.length; i++) {
    const id = orderedIds[i];
    const item = itemsById[id];

    // If we hit a non-tool item (next message), insert before it
    if (!id.startsWith("tool:")) {
      return i;
    }

    // Compare toolCallIndex — insert before tools with higher index
    const existingIndex = item?.toolCallIndex ?? 0;
    if (existingIndex > toolCallIndex) {
      return i;
    }
  }

  // No more messages or higher-index tools — append at end
  return orderedIds.length;
}

// ---- Ordered builder (for initial load / legacy) -------

/**
 * Build an ordered projection from an unsorted list of items.
 * Preserves the original transcript adjacency: tools stay near their parent
 * assistant messages. Items are ordered by their original array position.
 */
export function buildOrderedProjection(items: TimelineItem[]): TimelineProjection {
  const itemsById: Record<string, TimelineItem> = {};
  const orderedIds: string[] = [];

  for (const item of items) {
    itemsById[item.id] = item;
    orderedIds.push(item.id);
  }

  return {
    orderedIds,
    itemsById,
    lastAppliedSeqByRun: {},
    pendingTools: {},
  };
}
