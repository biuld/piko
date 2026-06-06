// ============================================================================
// Stream reducers — stream_started, assistant_delta, thinking_delta, queue_update
//
// Thinking deltas update the state.thinkingActive flag and accumulate
// thinking text in stream state for the status bar / thinking pill.
// They do NOT create separate timeline items — thinking is embedded
// in the assistant message in pi's UX.
// ============================================================================

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
      currentToolName: undefined,
      queueInfo: undefined,
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

  if (lastIdx >= 0) {
    const existingMsg = state.transcript[lastIdx];
    const updated = [...state.transcript];
    updated[lastIdx] = { ...existingMsg, text, isStreaming: true };

    const tlItemId = `msg:${existingMsg.id}`;
    const tlItems = updateStreamingTimelineItem(state.timeline.items, tlItemId, text);

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
    isStreaming: true,
  };
  const tlItem = createStreamingTimelineItem(msgId, text);
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
 * Thinking delta — accumulates thinking text in stream state but does NOT
 * create a timeline item. Like pi, thinking is shown in a thinking pill /
 * status bar indicator, not as a separate conversation entry.
 */
export function handleThinkingDelta(state: TuiState, event: ThinkingDeltaEvent): TuiState {
  return {
    ...state,
    stream: {
      ...state.stream,
      thinkingActive: true,
      thinkingText: (state.stream.thinkingText ?? "") + event.delta,
    },
  };
}

export function handleQueueUpdate(state: TuiState, event: QueueUpdateEvent): TuiState {
  const parts: string[] = [];
  if (event.steerCount > 0) {
    parts.push(`Steer:${event.steerCount}${event.steerPreview ? ` "${event.steerPreview}"` : ""}`);
  }
  if (event.followUpCount > 0) {
    parts.push(
      `FollowUp:${event.followUpCount}${event.followUpPreview ? ` "${event.followUpPreview}"` : ""}`,
    );
  }
  return {
    ...state,
    stream: {
      ...state.stream,
      queueInfo: parts.length > 0 ? parts.join(" │ ") : undefined,
    },
  };
}
