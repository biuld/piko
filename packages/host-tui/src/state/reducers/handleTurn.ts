// ============================================================================
// Turn reducers — turn_finished, turn_failed, aborted
// ============================================================================

import { buildTimelineItem } from "../../timeline/timeline-builder.js";
import { reconcileTranscript } from "../../timeline/transcript-reconcile.js";
import type { AbortedEvent, TurnFailedEvent, TurnFinishedEvent } from "../events.js";
import type { TuiMessageViewModel, TuiState } from "../state.js";
import { nextMessageId } from "./helpers.js";

export function handleTurnFinished(state: TuiState, event: TurnFinishedEvent): TuiState {
  const { transcript, timelineItems } = reconcileTranscript(
    event.transcript,
    state.transcript,
    state.timeline.items,
    { createMessageId: nextMessageId },
  );

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
      currentToolName: undefined,
      queueInfo: undefined,
    },
  };
}

export function handleTurnFailed(state: TuiState, event: TurnFailedEvent): TuiState {
  const errorMsg: TuiMessageViewModel = {
    id: nextMessageId(),
    role: "assistant",
    text: `Error: ${event.error}`,
  };
  const errorItem = buildTimelineItem(errorMsg);

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
      currentToolName: undefined,
      queueInfo: undefined,
    },
  };
}

export function handleAborted(state: TuiState, _event: AbortedEvent): TuiState {
  return {
    ...state,
    stream: { ...state.stream, status: "aborting" },
  };
}
