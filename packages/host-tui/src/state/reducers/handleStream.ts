// ============================================================================
// Stream reducers — stream_started, assistant_delta, thinking_delta, queue_update
//
// Thinking deltas update the state.thinkingActive flag and accumulate
// thinking text in stream state for the status bar / thinking pill.
// They do NOT create separate timeline items — thinking is embedded
// in the assistant message in pi's UX.
// ============================================================================

import type { QueueMessage } from "../../renderer/opentui/status/types.js";
import {
  createStreamingTimelineItem,
  updateStreamingTimelineItem,
} from "../../timeline/timeline-builder.js";
import type {
  AssistantDeltaEvent,
  QueueUpdateEvent,
  StreamStartedEvent,
  ThinkingDeltaEvent,
} from "../events.js";
import type { TuiMessageViewModel, TuiState } from "../state.js";
import { nextMessageId } from "./helpers.js";

export function handleStreamStarted(state: TuiState, _event: StreamStartedEvent): TuiState {
  return {
    ...state,
    stream: {
      ...state.stream,
      status: "running",
      assistantText: "",
      thinkingActive: false,
      thinkingText: "",
      currentToolCallId: undefined,
      queue: undefined,
    },
    timeline: {
      ...state.timeline,
      streamingItemId: undefined,
    },
  };
}

export function handleAssistantDelta(state: TuiState, event: AssistantDeltaEvent): TuiState {
  const text = state.stream.assistantText + event.delta;
  const thinkingText = state.stream.thinkingText || undefined;
  const streamingId = state.timeline.streamingItemId;

  if (streamingId) {
    const messageId = streamingId.startsWith("msg:") ? streamingId.slice(4) : streamingId;
    const idx = state.transcript.findIndex((m) => m.id === messageId);
    let updatedTranscript = state.transcript;
    if (idx >= 0) {
      const existingMsg = state.transcript[idx];
      const updated = [...state.transcript];
      updated[idx] = { ...existingMsg, text, thinkingText, isStreaming: true };
      updatedTranscript = updated;
    }

    const tlItems = updateStreamingTimelineItem(
      state.timeline.items,
      streamingId,
      text,
      thinkingText,
    );

    return {
      ...state,
      transcript: updatedTranscript,
      stream: { ...state.stream, assistantText: text },
      timeline: { ...state.timeline, items: tlItems },
    };
  }

  // No assistant message yet — create one
  const msgId = nextMessageId();
  const newMsg: TuiMessageViewModel = {
    id: msgId,
    role: "assistant",
    text,
    thinkingText,
    isStreaming: true,
  };
  const tlItem = createStreamingTimelineItem(msgId, text, thinkingText);
  const isManual = state.timeline.anchor === "manual";
  return {
    ...state,
    transcript: [...state.transcript, newMsg],
    timeline: {
      ...state.timeline,
      items: [...state.timeline.items, tlItem],
      streamingItemId: tlItem.id,
      pendingNewItems: isManual
        ? state.timeline.pendingNewItems + 1
        : state.timeline.pendingNewItems,
    },
    stream: { ...state.stream, assistantText: text },
  };
}

/**
 * Thinking delta — accumulates thinking text in stream state and updates
 * the streaming assistant timeline item so thinking renders inline.
 */
export function handleThinkingDelta(state: TuiState, event: ThinkingDeltaEvent): TuiState {
  const thinkingText = (state.stream.thinkingText ?? "") + event.delta;

  const streamingId = state.timeline.streamingItemId;
  let tlItems = state.timeline.items;
  let updatedTranscript = state.transcript;
  let nextStreamingId = streamingId;
  let pendingNewItems = state.timeline.pendingNewItems;

  if (streamingId) {
    tlItems = updateStreamingTimelineItem(
      state.timeline.items,
      streamingId,
      state.stream.assistantText,
      thinkingText,
    );
    const messageId = streamingId.startsWith("msg:") ? streamingId.slice(4) : streamingId;
    const idx = state.transcript.findIndex((m) => m.id === messageId);
    if (idx >= 0) {
      const existingMsg = state.transcript[idx];
      const updated = [...state.transcript];
      updated[idx] = { ...existingMsg, thinkingText, isStreaming: true };
      updatedTranscript = updated;
    }
  } else {
    // No assistant message or timeline item exists yet — create one
    const msgId = nextMessageId();
    const newMsg: TuiMessageViewModel = {
      id: msgId,
      role: "assistant",
      text: "",
      thinkingText,
      isStreaming: true,
    };
    const tlItem = createStreamingTimelineItem(msgId, "", thinkingText);
    tlItems = [...state.timeline.items, tlItem];
    updatedTranscript = [...state.transcript, newMsg];
    nextStreamingId = tlItem.id;
    const isManual = state.timeline.anchor === "manual";
    if (isManual) {
      pendingNewItems += 1;
    }
  }

  return {
    ...state,
    transcript: updatedTranscript,
    timeline: {
      ...state.timeline,
      items: tlItems,
      streamingItemId: nextStreamingId,
      pendingNewItems,
    },
    stream: {
      ...state.stream,
      thinkingActive: true,
      thinkingText,
    },
  };
}

export function handleQueueUpdate(state: TuiState, event: QueueUpdateEvent): TuiState {
  if (event.agentId && event.agentId !== state.currentAgentId) {
    return state;
  }
  const steering: QueueMessage[] = [];
  const followUp: QueueMessage[] = [];

  if (event.steerCount > 0 && event.steerPreview) {
    steering.push({ preview: event.steerPreview, content: event.steerPreview });
  }
  if (event.followUpCount > 0 && event.followUpPreview) {
    followUp.push({ preview: event.followUpPreview, content: event.followUpPreview });
  }

  const hasQueue = steering.length > 0 || followUp.length > 0;

  return {
    ...state,
    stream: {
      ...state.stream,
      queue: hasQueue ? { steering, followUp, nextTurnCount: 0 } : undefined,
    },
  };
}
