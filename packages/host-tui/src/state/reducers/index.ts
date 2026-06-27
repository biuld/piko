// ============================================================================
// Reducers barrel — all event handlers + root reducer
//
// Each handler lives in its own file. Add a new event:
//   1. Write the handler in the appropriate handle*.ts file
//   2. Add one entry to the `handlers` map below
// ============================================================================

import type { TuiEvent } from "../events.js";
import type { TuiState } from "../state.js";
import {
  handleEditorDraftChanged,
  handleEditorDraftReplaced,
  handleUserSubmitted,
} from "./handleInput.js";
import { handleChatScrolled, handleLayoutResized } from "./handleLayout.js";
import { handleModelChanged, handleThinkingLevelChanged } from "./handleModel.js";
import {
  handleSessionInfoUpdated,
  handleSessionResumed,
  handleTreeNavigationFailed,
  handleTreeNavigationStarted,
  handleTreeNavigationSucceeded,
} from "./handleSession.js";
import {
  handleAssistantDelta,
  handleMessageEnd,
  handleMessageStart,
  handleMessageUpdate,
  handleQueueUpdate,
  handleStreamStarted,
  handleThinkingDelta,
} from "./handleStream.js";

import {
  handleFocusChanged,
  handleNotificationAdded,
  handleNotificationCleared,
  handleNotificationRead,
  handleSurfaceClosed,
  handleSurfaceOpened,
  handleUsageUpdated,
} from "./handleSubsystems.js";
import { handleTimelineJumpLatest, handleTimelineToggleAllTools } from "./handleTimeline.js";
import {
  ensureToolForApproval,
  handleToolCallEnded,
  handleToolCallStarted,
} from "./handleToolCalls.js";
import { handleAborted, handleTurnFailed, handleTurnFinished } from "./handleTurn.js";

type Handler = (state: TuiState, event: any) => TuiState;

const handlers: Record<string, Handler> = {
  user_submitted: handleUserSubmitted,
  editor_draft_changed: handleEditorDraftChanged,
  editor_draft_replaced: handleEditorDraftReplaced,
  tree_navigation_started: handleTreeNavigationStarted,
  tree_navigation_succeeded: handleTreeNavigationSucceeded,
  tree_navigation_failed: handleTreeNavigationFailed,
  stream_started: handleStreamStarted,
  assistant_delta: handleAssistantDelta,
  thinking_delta: handleThinkingDelta,
  message_start: handleMessageStart,
  message_update: handleMessageUpdate,
  message_end: handleMessageEnd,
  tool_call_started: handleToolCallStarted,
  tool_call_ended: handleToolCallEnded,

  turn_finished: handleTurnFinished,
  turn_failed: handleTurnFailed,
  aborted: handleAborted,
  queue_update: handleQueueUpdate,
  layout_resized: handleLayoutResized,
  chat_scrolled: handleChatScrolled,
  model_changed: handleModelChanged,
  thinking_level_changed: handleThinkingLevelChanged,
  session_resumed: handleSessionResumed,
  session_info_updated: handleSessionInfoUpdated,
  usage_updated: handleUsageUpdated,
  notification_added: handleNotificationAdded,
  notification_cleared: handleNotificationCleared,
  notification_read: handleNotificationRead,
  surface_opened: handleSurfaceOpened,
  surface_closed: handleSurfaceClosed,
  timeline_jump_latest: handleTimelineJumpLatest,
  timeline_toggle_all_tools: handleTimelineToggleAllTools,
  focus_changed: handleFocusChanged,
  settings_updated: (state, event) => ({
    ...state,
    layout: {
      ...state.layout,
      ...event.settings,
    },
  }),
  viewed_agent_changed: (state, event) => ({
    ...state,
    viewedAgentId: event.agentId,
    expandedAgentId: undefined,
  }),
  agent_expansion_toggled: (state) => ({
    ...state,
    expandedAgentId: state.expandedAgentId ? undefined : state.viewedAgentId,
  }),
  approval_needed: (state, event) => {
    const nextState = ensureToolForApproval(state, event);
    const request = {
      toolEntityId: event.toolEntityId ?? event.callId,
      callId: event.callId,
      toolName: event.toolName,
      toolArgs: event.toolArgs,
    };
    const approval = nextState.approval;
    if (
      approval.pending?.toolEntityId === request.toolEntityId ||
      approval.queue.some((item) => item.toolEntityId === request.toolEntityId)
    ) {
      return nextState;
    }
    if (!approval.pending) {
      return {
        ...nextState,
        approval: { pending: request, queue: approval.queue },
        stream: {
          ...nextState.stream,
          status: "awaiting_approval",
          currentToolCallId: request.callId,
        },
      };
    }
    return {
      ...nextState,
      approval: { ...approval, queue: [...approval.queue, request] },
    };
  },
  approval_resolved: (state, event) => {
    const approval = state.approval;
    const entityId = event.toolEntityId ?? event.callId;
    if (approval.pending?.toolEntityId !== entityId) {
      const queue = approval.queue.filter((item) => item.toolEntityId !== entityId);
      return queue.length === approval.queue.length
        ? state
        : { ...state, approval: { ...approval, queue } };
    }
    const [pending, ...queue] = approval.queue;
    return {
      ...state,
      approval: { pending, queue },
      stream: {
        ...state.stream,
        status: pending ? "awaiting_approval" : "running",
        currentToolCallId: pending?.callId,
      },
    };
  },
  stream_settled: (state) => ({
    ...state,
    approval: { queue: [] },
    transcript: state.transcript.map((message) =>
      message.isStreaming ? { ...message, isStreaming: false } : message,
    ),
    timeline: {
      ...state.timeline,
      items: state.timeline.items.map((item) =>
        item.isStreaming ? { ...item, isStreaming: false } : item,
      ),
      streamingItemId: undefined,
    },
    stream: {
      ...state.stream,
      status: "idle",
      thinkingActive: false,
      currentToolCallId: undefined,
      queue: undefined,
    },
  }),
};

export function tuiReducer(state: TuiState, event: TuiEvent): TuiState {
  if (event.type === "surface_updated") {
    // Shallow-clone surfaces so render-plan WeakMap cache misses
    // and SolidJS For re-renders PanelRenderer with updated panel state.
    return {
      ...state,
      surfaces: state.surfaces.map((s) => ({ ...s })),
    };
  }
  const handler = handlers[event.type];
  return handler ? handler(state, event) : state;
}
