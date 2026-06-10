// ============================================================================
// Session reducers — session_resumed, session_info_updated
// ============================================================================

import { initTimelineItems } from "../../timeline/timeline-builder.js";
import type { SessionInfoUpdatedEvent, SessionResumedEvent } from "../events.js";
import type { TuiState } from "../state.js";
import { seedMessageIdSeq } from "./helpers.js";

export function handleSessionResumed(state: TuiState, event: SessionResumedEvent): TuiState {
  // Seed the message ID counter from existing transcript to avoid collisions
  const existingIds = event.transcript.map((m) => m.id);
  seedMessageIdSeq(existingIds);
  const items = initTimelineItems(event.transcript);
  const collapsedToolCallIds = new Set(
    items
      .filter(
        (item) => item.toolCallId && (item.toolStatus === "success" || item.toolStatus === "error"),
      )
      .map((item) => item.toolCallId!),
  );

  return {
    ...state,
    session: {
      ...state.session,
      sessionId: event.sessionId,
      sessionName: event.sessionName ?? state.session.sessionName,
      messageCount: event.transcript.length,
    },
    transcript: event.transcript,
    timeline: {
      ...state.timeline,
      items,
      collapsedToolCallIds,
    },
    stream: { ...state.stream, status: "idle" },
  };
}

export function handleSessionInfoUpdated(
  state: TuiState,
  event: SessionInfoUpdatedEvent,
): TuiState {
  return {
    ...state,
    session: {
      ...state.session,
      sessionId: event.sessionId ?? state.session.sessionId,
      sessionName: event.sessionName ?? state.session.sessionName,
      messageCount: event.messageCount ?? state.session.messageCount,
    },
  };
}
