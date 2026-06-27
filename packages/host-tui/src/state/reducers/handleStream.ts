// ============================================================================
// Stream reducers — stream_started, assistant_delta, thinking_delta, queue_update
//
// Thinking deltas update the state.thinkingActive flag and accumulate
// thinking text in stream state for the status bar / thinking pill.
// They do NOT create separate timeline items — thinking is embedded
// in the assistant message in pi's UX.
//
// Message lifecycle reducers (message_start/update/end) use the
// TimelineProjection for deterministic, ID-keyed ordering.
// Messages are append-only; tools are inserted after their parent.
// ============================================================================

import type { QueueMessage } from "../../renderer/opentui/status/types.js";
import type { RuntimeMessage } from "../../shared/index.js";
import { materializeProjection, upsertAssistantMessage } from "../../timeline/projection.js";
import {
  createStreamingTimelineItem,
  updateStreamingTimelineItem,
} from "../../timeline/timeline-builder.js";
import type {
  AssistantDeltaEvent,
  MessageEndEvent,
  MessageStartEvent,
  MessageUpdateEvent,
  QueueUpdateEvent,
  StreamStartedEvent,
  ThinkingDeltaEvent,
} from "../events.js";
import type { TuiMessageViewModel, TuiState } from "../state.js";
import { applySeq } from "./diagnostics.js";
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

    // Also update the projection item
    const proj = { ...state.projection };
    if (streamingId in proj.itemsById) {
      const item = proj.itemsById[streamingId];
      proj.itemsById = {
        ...proj.itemsById,
        [streamingId]: { ...item, text, thinkingText, isStreaming: true },
      };
    }

    return {
      ...state,
      transcript: updatedTranscript,
      stream: { ...state.stream, assistantText: text },
      timeline: { ...state.timeline, items: tlItems },
      projection: proj,
    };
  }

  // No assistant message yet — create one (legacy path for token events)
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

  const proj = upsertAssistantMessage(state.projection, tlItem);

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
    projection: proj,
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
  let proj = state.projection;

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
    if (streamingId in proj.itemsById) {
      const item = proj.itemsById[streamingId];
      proj = {
        ...proj,
        itemsById: {
          ...proj.itemsById,
          [streamingId]: { ...item, thinkingText, isStreaming: true },
        },
      };
    }
  } else {
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
    proj = upsertAssistantMessage(proj, tlItem);
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
    projection: proj,
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

export function extractTextAndThinking(message: RuntimeMessage): {
  text: string;
  thinkingText?: string;
} {
  let text = "";
  let thinkingText = "";
  if (message.role === "assistant") {
    for (const block of message.content) {
      if (block.type === "text") {
        text += block.text;
      } else if (block.type === "thinking") {
        thinkingText += block.thinking;
      }
    }
  } else if (message.role === "user") {
    for (const block of message.content) {
      if (block.type === "text") {
        text += block.text;
      }
    }
  } else if (message.role === "toolResult") {
    text =
      typeof message.content === "string"
        ? message.content
        : JSON.stringify(message.content, null, 2);
  }
  return {
    text,
    thinkingText: thinkingText || undefined,
  };
}

export function handleMessageStart(state: TuiState, event: MessageStartEvent): TuiState {
  // Idempotency: if projection already has this message, skip duplicate insertion
  const msgId = `msg:${event.message.id}`;
  const alreadyExists = msgId in state.projection.itemsById;

  const { message } = event;
  const { text, thinkingText } = extractTextAndThinking(message);

  // Sequence validation
  const runId = event.runId ?? `msg_${message.id}`;
  let proj = applySeq(state.projection, runId, event.eventSeq);

  const newMsg: TuiMessageViewModel = {
    id: message.id,
    role: message.role as any,
    text,
    thinkingText,
    isStreaming: true,
    message,
    content: message.role === "assistant" ? message.content : undefined,
  };

  let kind: any = "assistant-stream";
  if (message.role === "user") {
    kind = "user-message";
  } else if (message.role === "toolResult") {
    kind = "tool-result";
  }

  const tlItem: import("../../timeline/types.js").TimelineItem = {
    id: msgId,
    kind,
    role: message.role as any,
    text,
    thinkingText,
    messageId: message.id,
    isStreaming: true,
    createdAt: state.projection.itemsById[msgId]?.createdAt ?? Date.now(),
    message,
    content: message.role === "assistant" ? message.content : undefined,
    turnIndex: event.turnIndex,
    eventSeq: event.eventSeq,
    messageIndex: event.messageIndex,
    runId,
    data: newMsg,
  };

  proj = upsertAssistantMessage(proj, tlItem);

  const updatedItems = materializeProjection(proj);

  const updatedTranscript = [...state.transcript];
  const transIdx = updatedTranscript.findIndex((m) => m.id === message.id);
  if (transIdx >= 0) {
    updatedTranscript[transIdx] = newMsg;
  } else {
    updatedTranscript.push(newMsg);
  }

  const isManual = state.timeline.anchor === "manual";
  return {
    ...state,
    transcript: updatedTranscript,
    timeline: {
      ...state.timeline,
      items: updatedItems,
      streamingItemId: tlItem.id,
      pendingNewItems:
        isManual && !alreadyExists
          ? state.timeline.pendingNewItems + 1
          : state.timeline.pendingNewItems,
    },
    stream: {
      ...state.stream,
      status: "running",
      assistantText: text,
      thinkingText: thinkingText ?? "",
      thinkingActive: thinkingText !== undefined && thinkingText.length > 0,
    },
    projection: proj,
  };
}

export function handleMessageUpdate(state: TuiState, event: MessageUpdateEvent): TuiState {
  const { message, assistantEvent } = event;
  const { text, thinkingText } = extractTextAndThinking(message);

  // Sequence validation
  const runId = event.runId ?? `msg_${message.id}`;
  let proj = applySeq(state.projection, runId, event.eventSeq);

  const streamingId = `msg:${message.id}`;
  const isThinking = assistantEvent?.type.startsWith("thinking");

  const isAssistant = message.role === "assistant";
  const isStreaming =
    isAssistant && assistantEvent?.type !== "done" && assistantEvent?.type !== "error";
  const isError =
    (isAssistant && message.stopReason === "error") || assistantEvent?.type === "error";
  const errorMessage =
    assistantEvent?.type === "error"
      ? assistantEvent.message
      : isAssistant
        ? message.errorMessage
        : undefined;

  const updatedTranscript = [...state.transcript];
  const idx = updatedTranscript.findIndex((m) => m.id === message.id);
  const newMsg: TuiMessageViewModel = {
    id: message.id,
    role: message.role as any,
    text,
    thinkingText,
    isStreaming,
    isError,
    errorMessage,
    message,
    content: message.role === "assistant" ? message.content : undefined,
  };
  if (idx >= 0) {
    updatedTranscript[idx] = newMsg;
  } else {
    updatedTranscript.push(newMsg);
  }

  const existingItem = proj.itemsById[streamingId];
  const tlItem: import("../../timeline/types.js").TimelineItem = {
    id: streamingId,
    kind: "assistant-stream" as const,
    role: message.role as any,
    text,
    thinkingText,
    isError,
    errorMessage,
    messageId: message.id,
    isStreaming,
    createdAt: existingItem?.createdAt ?? Date.now(),
    message,
    content: message.role === "assistant" ? message.content : undefined,
    turnIndex: event.turnIndex,
    eventSeq: event.eventSeq,
    messageIndex: event.messageIndex,
    runId,
    data: newMsg,
  };
  // Any lifecycle event may be the first one observed. Upsert rather than
  // requiring message_start, and re-parent tools waiting for this message.
  proj = upsertAssistantMessage(proj, tlItem);

  return {
    ...state,
    transcript: updatedTranscript,
    timeline: {
      ...state.timeline,
      items: materializeProjection(proj),
      streamingItemId: streamingId,
    },
    stream: {
      ...state.stream,
      assistantText: text,
      thinkingText: thinkingText ?? "",
      thinkingActive: isThinking ?? state.stream.thinkingActive,
    },
    projection: proj,
  };
}

export function handleMessageEnd(state: TuiState, event: MessageEndEvent): TuiState {
  const { message } = event;
  const { text, thinkingText } = extractTextAndThinking(message);

  // Sequence validation
  const runId = event.runId ?? `msg_${message.id}`;
  let proj = applySeq(state.projection, runId, event.eventSeq);

  const streamingId = `msg:${message.id}`;

  const isAssistant = message.role === "assistant";
  const isError = isAssistant && message.stopReason === "error";
  const errorMessage = isAssistant ? message.errorMessage : undefined;

  const updatedTranscript = [...state.transcript];
  const idx = updatedTranscript.findIndex((m) => m.id === message.id);
  const newMsg: TuiMessageViewModel = {
    id: message.id,
    role: message.role as any,
    text,
    thinkingText,
    isStreaming: false,
    isError,
    errorMessage,
    message,
    content: message.role === "assistant" ? message.content : undefined,
  };
  if (idx >= 0) {
    updatedTranscript[idx] = newMsg;
  } else {
    updatedTranscript.push(newMsg);
  }

  let kind: any = "assistant-message";
  if (message.role === "user") kind = "user-message";
  else if (message.role === "toolResult") kind = "tool-result";
  const existingItem = proj.itemsById[streamingId];
  proj = upsertAssistantMessage(proj, {
    ...existingItem,
    id: streamingId,
    kind,
    role: message.role as any,
    text,
    thinkingText,
    isError,
    errorMessage,
    messageId: message.id,
    isStreaming: false,
    createdAt: existingItem?.createdAt ?? Date.now(),
    message,
    content: message.role === "assistant" ? message.content : undefined,
    turnIndex: event.turnIndex,
    eventSeq: event.eventSeq,
    messageIndex: event.messageIndex,
    runId,
    data: newMsg,
  });

  return {
    ...state,
    transcript: updatedTranscript,
    timeline: {
      ...state.timeline,
      items: materializeProjection(proj),
      streamingItemId: undefined,
    },
    stream: {
      ...state.stream,
      thinkingActive: false,
    },
    projection: proj,
  };
}
