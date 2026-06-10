// ============================================================================
// Tool call reducers — tool_call_started, tool_call_ended
// ============================================================================

import { buildTimelineItem } from "../../timeline/timeline-builder.js";
import type { ToolCallEndedEvent, ToolCallStartedEvent } from "../events.js";
import type { TuiMessageViewModel, TuiState } from "../state.js";
import { findToolCallIndex, nextMessageId } from "./helpers.js";

export function handleToolCallStarted(state: TuiState, event: ToolCallStartedEvent): TuiState {
  const wasManual = state.timeline.anchor === "manual";
  const toolMsg: TuiMessageViewModel = {
    id: nextMessageId(),
    role: "tool",
    text: "",
    toolBlock: {
      toolCallId: event.id,
      name: event.name,
      args: event.args,
      status: "running",
      isCollapsed: false,
    },
  };
  const tlItem = buildTimelineItem(toolMsg);
  return {
    ...state,
    transcript: [...state.transcript, toolMsg],
    timeline: {
      ...state.timeline,
      items: [...state.timeline.items, tlItem],
      pendingNewItems: wasManual
        ? state.timeline.pendingNewItems + 1
        : state.timeline.pendingNewItems,
    },
    stream: {
      ...state.stream,
      currentToolCallId: event.id,
    },
  };
}

export function handleToolCallEnded(state: TuiState, event: ToolCallEndedEvent): TuiState {
  const toolIdx = findToolCallIndex(state.transcript, event.id);
  if (toolIdx < 0) return state;

  const updated = [...state.transcript];
  updated[toolIdx] = {
    ...updated[toolIdx],
    toolBlock: {
      ...updated[toolIdx].toolBlock!,
      status: event.isError ? "error" : "success",
      result: event.result,
    },
  };

  // Update the tool timeline item by stable toolCallId
  const tlItemId = `tool:${event.id}`;
  const tlItems = [...state.timeline.items];
  const tlIdx = tlItems.findIndex((i) => i.id === tlItemId);
  if (tlIdx >= 0) {
    tlItems[tlIdx] = {
      ...tlItems[tlIdx],
      kind: "tool-result",
      toolStatus: event.isError ? "error" : "success",
      toolResult: event.result,
    };
  }

  return {
    ...state,
    transcript: updated,
    timeline: {
      ...state.timeline,
      items: tlItems,
      collapsedToolCallIds: new Set([...state.timeline.collapsedToolCallIds, event.id]),
    },
    stream: {
      ...state.stream,
      currentToolCallId: undefined,
    },
  };
}
