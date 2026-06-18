// ============================================================================
// Stream reducers — stream_started, assistant_delta, thinking_delta, queue_update
//
// Thinking deltas update the state.thinkingActive flag and accumulate
// thinking text in stream state for the status bar / thinking pill.
// They do NOT create separate timeline items — thinking is embedded
// in the assistant message in pi's UX.
// ============================================================================

import type { RuntimeMessage } from "piko-orchestrator-protocol";
import type { QueueMessage } from "../../renderer/opentui/status/types.js";
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
  const { message } = event;
  const { text, thinkingText } = extractTextAndThinking(message);

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

  const tlItem = {
    id: `msg:${message.id}`,
    kind,
    role: message.role as any,
    text,
    thinkingText,
    messageId: message.id,
    isStreaming: true,
    createdAt: Date.now(),
    message,
    content: message.role === "assistant" ? message.content : undefined,
    data: newMsg,
  };

  const isManual = state.timeline.anchor === "manual";
  const updatedItems = [...state.timeline.items];
  const existingIdx = updatedItems.findIndex((i) => i.id === tlItem.id);
  if (existingIdx >= 0) {
    updatedItems[existingIdx] = tlItem;
  } else {
    updatedItems.push(tlItem);
  }

  const updatedTranscript = [...state.transcript];
  const transIdx = updatedTranscript.findIndex((m) => m.id === message.id);
  if (transIdx >= 0) {
    updatedTranscript[transIdx] = newMsg;
  } else {
    updatedTranscript.push(newMsg);
  }

  return {
    ...state,
    transcript: updatedTranscript,
    timeline: {
      ...state.timeline,
      items: updatedItems,
      streamingItemId: tlItem.id,
      pendingNewItems:
        isManual && existingIdx < 0
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
  };
}

export function handleMessageUpdate(state: TuiState, event: MessageUpdateEvent): TuiState {
  const { message, assistantEvent } = event;
  const { text, thinkingText } = extractTextAndThinking(message);

  const streamingId = `msg:${message.id}`;
  const isThinking = assistantEvent?.type.startsWith("thinking");

  const updatedTranscript = [...state.transcript];
  const idx = updatedTranscript.findIndex((m) => m.id === message.id);
  const newMsg: TuiMessageViewModel = {
    id: message.id,
    role: message.role as any,
    text,
    thinkingText,
    isStreaming: true,
    message,
    content: message.role === "assistant" ? message.content : undefined,
  };
  if (idx >= 0) {
    updatedTranscript[idx] = newMsg;
  } else {
    updatedTranscript.push(newMsg);
  }

  const updatedItems = [...state.timeline.items];
  const tlIdx = updatedItems.findIndex((i) => i.id === streamingId);
  const tlItem = {
    id: streamingId,
    kind: "assistant-stream" as const,
    role: message.role as any,
    text,
    thinkingText,
    messageId: message.id,
    isStreaming: true,
    createdAt: tlIdx >= 0 ? updatedItems[tlIdx].createdAt : Date.now(),
    message,
    content: message.role === "assistant" ? message.content : undefined,
    data: newMsg,
  };
  if (tlIdx >= 0) {
    updatedItems[tlIdx] = tlItem;
  } else {
    updatedItems.push(tlItem);
  }

  return {
    ...state,
    transcript: updatedTranscript,
    timeline: {
      ...state.timeline,
      items: updatedItems,
      streamingItemId: streamingId,
    },
    stream: {
      ...state.stream,
      assistantText: text,
      thinkingText: thinkingText ?? "",
      thinkingActive: isThinking ?? state.stream.thinkingActive,
    },
  };
}

export function handleMessageEnd(state: TuiState, event: MessageEndEvent): TuiState {
  const { message } = event;
  const { text, thinkingText } = extractTextAndThinking(message);

  const streamingId = `msg:${message.id}`;

  const updatedTranscript = [...state.transcript];
  const idx = updatedTranscript.findIndex((m) => m.id === message.id);
  const newMsg: TuiMessageViewModel = {
    id: message.id,
    role: message.role as any,
    text,
    thinkingText,
    isStreaming: false,
    message,
    content: message.role === "assistant" ? message.content : undefined,
  };
  if (idx >= 0) {
    updatedTranscript[idx] = newMsg;
  } else {
    updatedTranscript.push(newMsg);
  }

  const updatedItems = [...state.timeline.items];
  const tlIdx = updatedItems.findIndex((i) => i.id === streamingId);
  if (tlIdx >= 0) {
    let kind: any = "assistant-message";
    if (message.role === "user") {
      kind = "user-message";
    } else if (message.role === "toolResult") {
      kind = "tool-result";
    }

    updatedItems[tlIdx] = {
      ...updatedItems[tlIdx],
      kind,
      isStreaming: false,
      text,
      thinkingText,
      message,
      content: message.role === "assistant" ? message.content : undefined,
      data: newMsg,
    };
  }

  return {
    ...state,
    transcript: updatedTranscript,
    timeline: {
      ...state.timeline,
      items: updatedItems,
      streamingItemId: undefined,
    },
    stream: {
      ...state.stream,
      thinkingActive: false,
    },
  };
}
