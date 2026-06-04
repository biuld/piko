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
import {
  handleChatScrolled,
  handleLayoutResized,
  handleRegionFocused,
  handleToolBlockToggled,
} from "./handleLayout.js";
import { handleModelChanged, handleThinkingLevelChanged } from "./handleModel.js";
import {
  handleSessionForked,
  handleSessionInfoUpdated,
  handleSessionResumed,
} from "./handleSession.js";
import {
  handleAssistantDelta,
  handleQueueUpdate,
  handleStreamStarted,
  handleThinkingDelta,
} from "./handleStream.js";
import {
  handleExtensionStatusSet,
  handleFocusChanged,
  handleNotificationAdded,
  handleNotificationCleared,
  handleNotificationRead,
  handleSurfaceClosed,
  handleSurfaceOpened,
  handleUsageUpdated,
} from "./handleSubsystems.js";
import {
  handleTimelineItemToggled,
  handleTimelineJumpLatest,
  handleTimelinePendingUpdate,
  handleTimelineScrolled,
  handleTimelineToggleAllTools,
  handleTimelineToolToggled,
} from "./handleTimeline.js";
import { handleToolCallEnded, handleToolCallStarted } from "./handleToolCalls.js";
import { handleAborted, handleTurnFailed, handleTurnFinished } from "./handleTurn.js";

type Handler = (state: TuiState, event: any) => TuiState;

const handlers: Record<string, Handler> = {
  user_submitted: handleUserSubmitted,
  stream_started: handleStreamStarted,
  assistant_delta: handleAssistantDelta,
  thinking_delta: handleThinkingDelta,
  tool_call_started: handleToolCallStarted,
  tool_call_ended: handleToolCallEnded,
  turn_finished: handleTurnFinished,
  turn_failed: handleTurnFailed,
  aborted: handleAborted,
  queue_update: handleQueueUpdate,
  layout_resized: handleLayoutResized,
  region_focused: handleRegionFocused,
  chat_scrolled: handleChatScrolled,
  tool_block_toggled: handleToolBlockToggled,
  model_changed: handleModelChanged,
  thinking_level_changed: handleThinkingLevelChanged,
  session_resumed: handleSessionResumed,
  session_forked: handleSessionForked,
  session_info_updated: handleSessionInfoUpdated,
  usage_updated: handleUsageUpdated,
  extension_status_set: handleExtensionStatusSet,
  notification_added: handleNotificationAdded,
  notification_cleared: handleNotificationCleared,
  notification_read: handleNotificationRead,
  surface_opened: handleSurfaceOpened,
  surface_closed: handleSurfaceClosed,
  timeline_scrolled: handleTimelineScrolled,
  timeline_jump_latest: handleTimelineJumpLatest,
  timeline_item_toggled: handleTimelineItemToggled,
  timeline_tool_toggled: handleTimelineToolToggled,
  timeline_pending_update: handleTimelinePendingUpdate,
  timeline_toggle_all_tools: handleTimelineToggleAllTools,
  focus_changed: handleFocusChanged,
};

export function tuiReducer(state: TuiState, event: TuiEvent): TuiState {
  const handler = handlers[event.type];
  return handler ? handler(state, event) : state;
}
