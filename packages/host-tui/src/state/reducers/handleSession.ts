// ============================================================================
// Session reducers — session_resumed, session_forked, session_info_updated
// ============================================================================

import { initTimelineItems } from "../../timeline/timeline-builder.js";
import type {
  SessionForkedEvent,
  SessionInfoUpdatedEvent,
  SessionResumedEvent,
} from "../events.js";
import type { TuiState } from "../state.js";
import { seedMessageIdSeq } from "./helpers.js";

export function handleSessionResumed(state: TuiState, event: SessionResumedEvent): TuiState {
  // Seed the message ID counter from existing transcript to avoid collisions
  const existingIds = event.transcript.map((m) => m.id);
  seedMessageIdSeq(existingIds);

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
      items: initTimelineItems(event.transcript),
    },
    stream: { ...state.stream, status: "idle" },
  };
}

export function handleSessionForked(state: TuiState, event: SessionForkedEvent): TuiState {
  return {
    ...state,
    session: {
      ...state.session,
      sessionId: event.sessionId,
    },
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
