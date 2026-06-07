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
import { findLastAssistantIndex, nextMessageId } from "./helpers.js";

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
  const lastIdx = findLastAssistantIndex(state.transcript);
  const text = state.stream.assistantText + event.delta;
  const thinkingText = state.stream.thinkingText || undefined;

  if (lastIdx >= 0) {
    const existingMsg = state.transcript[lastIdx];
    const updated = [...state.transcript];
    updated[lastIdx] = { ...existingMsg, text, thinkingText, isStreaming: true };

    const tlItemId = `msg:${existingMsg.id}`;
    const tlItems = updateStreamingTimelineItem(state.timeline.items, tlItemId, text, thinkingText);

    return {
      ...state,
      transcript: updated,
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

  // Also update the streaming timeline item's thinkingText
  const streamingId = state.timeline.streamingItemId;
  let tlItems = state.timeline.items;
  if (streamingId) {
    tlItems = updateStreamingTimelineItem(
      state.timeline.items,
      streamingId,
      state.stream.assistantText,
      thinkingText,
    );
  }

  return {
    ...state,
    timeline: { ...state.timeline, items: tlItems },
    stream: {
      ...state.stream,
      thinkingActive: true,
      thinkingText,
    },
  };
}

export function handleQueueUpdate(state: TuiState, event: QueueUpdateEvent): TuiState {
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
