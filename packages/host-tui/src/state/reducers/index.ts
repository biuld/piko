// ============================================================================
// Reducers barrel — all event handlers + root reducer
//
// Each handler lives in its own file. Add a new event:
//   1. Write the handler in the appropriate handle*.ts file
//   2. Add one entry to the `handlers` map below
// ============================================================================

import type { TuiEvent } from "../events.js";
import type { TuiState } from "../state.js";
import { handleUserSubmitted } from "./handleInput.js";
import { handleChatScrolled, handleLayoutResized } from "./handleLayout.js";
import { handleModelChanged, handleThinkingLevelChanged } from "./handleModel.js";
import { handleSessionInfoUpdated, handleSessionResumed } from "./handleSession.js";
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
import { handleToolCallEnded, handleToolCallStarted } from "./handleToolCalls.js";
import { handleAborted, handleTurnFailed, handleTurnFinished } from "./handleTurn.js";

type Handler = (state: TuiState, event: any) => TuiState;

const handlers: Record<string, Handler> = {
  user_submitted: handleUserSubmitted,
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
