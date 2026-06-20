import { buildOrderedProjection } from "../../timeline/projection.js";
import { initTimelineItems } from "../../timeline/timeline-builder.js";
import type {
  SessionInfoUpdatedEvent,
  SessionResumedEvent,
  TreeNavigationFailedEvent,
  TreeNavigationStartedEvent,
  TreeNavigationSucceededEvent,
} from "../events.js";
import type { TuiState } from "../state.js";
import { seedMessageIdSeq } from "./helpers.js";

export function handleSessionResumed(state: TuiState, event: SessionResumedEvent): TuiState {
  // Seed the message ID counter from existing transcript to avoid collisions
  const existingIds = event.transcript.map((m) => m.id);
  seedMessageIdSeq(existingIds);
  const items = initTimelineItems(event.transcript);

  // Build projection from all items — tools are positioned after their parent messages
  const projection = buildOrderedProjection(items);

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
      navigation: { status: "idle" },
    },
    transcript: event.transcript,
    timeline: {
      ...state.timeline,
      items,
      collapsedToolCallIds,
    },
    stream: { ...state.stream, status: "idle" },
    projection,
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

export function handleTreeNavigationStarted(
  state: TuiState,
  event: TreeNavigationStartedEvent,
): TuiState {
  return {
    ...state,
    session: {
      ...state.session,
      navigation: {
        status: "running",
        operationId: event.operationId,
        entryId: event.entryId,
      },
    },
  };
}

export function handleTreeNavigationSucceeded(
  state: TuiState,
  event: TreeNavigationSucceededEvent,
): TuiState {
  if (state.session.navigation.operationId !== event.operationId) {
    return state;
  }

  const items = initTimelineItems(event.result.transcript);

  // Build projection from all items — tools are positioned after their parent messages
  const projection = buildOrderedProjection(items);

  const collapsedToolCallIds = new Set(
    items
      .filter(
        (item) => item.toolCallId && (item.toolStatus === "success" || item.toolStatus === "error"),
      )
      .map((item) => item.toolCallId!),
  );

  let inputState = state.input;
  if (event.result.editorDraft) {
    inputState = {
      ...state.input,
      draft: event.result.editorDraft.text,
      revision: event.result.editorDraft.revision,
      source: event.result.editorDraft.source,
    };
  }

  return {
    ...state,
    session: {
      ...state.session,
      sessionId: event.result.sessionId,
      messageCount: event.result.transcript.length,
      navigation: { status: "idle" },
    },
    transcript: event.result.transcript,
    timeline: {
      ...state.timeline,
      items,
      collapsedToolCallIds,
    },
    input: inputState,
    projection,
  };
}

export function handleTreeNavigationFailed(
  state: TuiState,
  event: TreeNavigationFailedEvent,
): TuiState {
  if (state.session.navigation.operationId !== event.operationId) {
    return state;
  }

  return {
    ...state,
    session: {
      ...state.session,
      navigation: {
        status: "failed",
        error: event.error,
        operationId: state.session.navigation.operationId,
        entryId: state.session.navigation.entryId,
      },
    },
  };
}
