// ============================================================================
// Turn reducers — turn_finished, turn_failed, aborted
//
// Uses TimelineProjection for deterministic finalization.
// The projection already has the correct order from live events,
// so finalization validates and fills in missing canonical content.
// ============================================================================

import { buildTimelineItem } from "../../timeline/timeline-builder.js";
import {
  finalizeProjection,
  reconcileLegacyTranscript,
} from "../../timeline/transcript-reconcile.js";
import type { AbortedEvent, TurnFailedEvent, TurnFinishedEvent } from "../events.js";
import type { TuiMessageViewModel, TuiState } from "../state.js";
import { nextMessageId } from "./helpers.js";

export function handleTurnFinished(state: TuiState, event: TurnFinishedEvent): TuiState {
  // Determine if we have runtime ordering (live session) or legacy
  const hasRuntimeOrdering =
    state.projection.orderedIds.length > 0 &&
    state.projection.orderedIds.some((id) => {
      const item = state.projection.itemsById[id];
      return item?.eventSeq !== undefined && item.eventSeq >= 0;
    });

  let projection = state.projection;
  let transcript = state.transcript;
  let timelineItems = state.timeline.items;

  if (hasRuntimeOrdering) {
    // Live finalization: validate + complete the existing projection
    const result = finalizeProjection(projection, event.transcript);
    projection = result.projection;

    // Rebuild transcript from finalized projection
    transcript = buildTranscriptFromProjection(projection, state.transcript);
    timelineItems = projection.orderedIds.map((id) => projection.itemsById[id]).filter(Boolean);
  } else {
    // Legacy reconciliation path (no runtime IDs)
    const result = reconcileLegacyTranscript(
      event.transcript,
      state.transcript,
      state.timeline.items,
      { createMessageId: nextMessageId },
    );
    transcript = result.transcript;
    timelineItems = result.timelineItems;
  }

  return {
    ...state,
    transcript,
    timeline: {
      ...state.timeline,
      items: timelineItems,
      streamingItemId: undefined,
      pendingNewItems: 0,
    },
    stream: {
      ...state.stream,
      status: "idle" as const,
      assistantText: "",
      thinkingActive: false,
      thinkingText: "",
      currentToolCallId: undefined,
      queue: undefined,
    },
    projection,
  };
}

export function handleTurnFailed(state: TuiState, event: TurnFailedEvent): TuiState {
  const msgId = nextMessageId();
  const errorMsg: TuiMessageViewModel = {
    id: msgId,
    role: "assistant",
    text: `Error: ${event.error}`,
  };
  const errorItem = buildTimelineItem(errorMsg);

  // Also insert error into projection at end
  const messageIndex = state.transcript.length;
  const proj = { ...state.projection };
  const tlId = `msg:${msgId}`;
  const timelineItem = {
    ...errorItem,
    messageIndex,
    data: errorMsg,
  };
  proj.itemsById = { ...proj.itemsById, [tlId]: timelineItem };
  proj.orderedIds = [...proj.orderedIds, tlId];

  return {
    ...state,
    transcript: [...state.transcript, errorMsg],
    timeline: {
      ...state.timeline,
      items: [...state.timeline.items, errorItem],
      streamingItemId: undefined,
      pendingNewItems: state.timeline.anchor === "manual" ? state.timeline.pendingNewItems + 1 : 0,
    },
    stream: {
      ...state.stream,
      status: "idle",
      assistantText: "",
      thinkingActive: false,
      thinkingText: "",
      currentToolCallId: undefined,
      queue: undefined,
    },
    projection: proj,
  };
}

export function handleAborted(state: TuiState, _event: AbortedEvent): TuiState {
  return {
    ...state,
    stream: { ...state.stream, status: "aborting" },
  };
}

/** Rebuild TuiMessageViewModel[] from the projection. */
function buildTranscriptFromProjection(
  proj: import("../../timeline/projection.js").TimelineProjection,
  existingTranscript: TuiMessageViewModel[],
): TuiMessageViewModel[] {
  const existingById = new Map(existingTranscript.map((m) => [m.id, m]));
  const result: TuiMessageViewModel[] = [];
  for (const id of proj.orderedIds) {
    const item = proj.itemsById[id];
    if (!item) continue;
    const messageId = item.messageId ?? (item.id.startsWith("msg:") ? item.id.slice(4) : item.id);
    const existing = existingById.get(messageId);

    if (existing) {
      result.push({
        ...existing,
        text: item.text ?? existing.text,
        thinkingText: item.thinkingText ?? existing.thinkingText,
        isStreaming: false,
        message: item.message ?? existing.message,
        content: item.content ?? existing.content,
      });
    } else if (item.id.startsWith("tool:")) {
      result.push({
        id: messageId,
        role: "tool",
        text: item.text ?? "",
        toolBlock: {
          toolCallId: item.toolCallId ?? messageId,
          name: item.toolName ?? "tool",
          args: item.toolArgs ?? {},
          status: item.toolStatus ?? "success",
          result: item.toolResult,
          isCollapsed: false,
        },
      });
    } else {
      result.push({
        id: messageId,
        role: (item.role as any) ?? "assistant",
        text: item.text ?? "",
        thinkingText: item.thinkingText,
        isStreaming: false,
        message: item.message,
        content: item.content,
      });
    }
  }
  return result;
}
