// ============================================================================
// Tool call reducers — tool_call_started, tool_call_ended
//
// Tools are positioned after their parent assistant message,
// ordered by toolCallIndex. Uses TimelineProjection for deterministic ordering.
// Sequence validation applies on both start and end events.
// ============================================================================

import { upsertToolItem } from "../../timeline/projection.js";
import { buildTimelineItem } from "../../timeline/timeline-builder.js";
import type { ToolCallEndedEvent, ToolCallStartedEvent } from "../events.js";
import type { TuiMessageViewModel, TuiState } from "../state.js";
import { applySeq } from "./diagnostics.js";
import { findToolCallIndex, nextMessageId } from "./helpers.js";

export function handleToolCallStarted(state: TuiState, event: ToolCallStartedEvent): TuiState {
  // Idempotency check: if projection already has this tool, skip duplicate insertion
  const toolItemId = `tool:${event.id}`;
  if (toolItemId in state.projection.itemsById) {
    // Duplicate event — update projection only (no new transcript/timeline entries)
    const proj = applySeq(state.projection, event.runId ?? `tool_${event.id}`, event.eventSeq);
    return {
      ...state,
      stream: { ...state.stream, currentToolCallId: event.id },
      projection: proj,
    };
  }

  const wasManual = state.timeline.anchor === "manual";

  // Sequence validation
  const runId = event.runId ?? `tool_${event.id}`;
  let proj = applySeq(state.projection, runId, event.eventSeq);

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

  const tlItem = {
    ...buildTimelineItem(toolMsg),
    parentMessageId: event.parentMessageId,
    contentIndex: event.contentIndex,
    toolCallIndex: event.toolCallIndex,
    turnIndex: event.turnIndex,
    eventSeq: event.eventSeq,
  };

  // Insert tool after its parent assistant, ordered by toolCallIndex
  proj = upsertToolItem(proj, tlItem, event.parentMessageId ?? "", event.toolCallIndex ?? 0);

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
    projection: proj,
  };
}

export function handleToolCallEnded(state: TuiState, event: ToolCallEndedEvent): TuiState {
  // Sequence validation
  const runId = event.runId ?? `tool_${event.id}`;
  let proj = applySeq(state.projection, runId, event.eventSeq);

  const toolIdx = findToolCallIndex(state.transcript, event.id);

  let updatedTranscript = state.transcript;
  if (toolIdx >= 0) {
    updatedTranscript = [...state.transcript];
    updatedTranscript[toolIdx] = {
      ...updatedTranscript[toolIdx],
      toolBlock: {
        ...updatedTranscript[toolIdx].toolBlock!,
        status: event.isError ? "error" : "success",
        result: event.result,
      },
    };
  }

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
      parentMessageId: event.parentMessageId ?? tlItems[tlIdx].parentMessageId,
      contentIndex: event.contentIndex ?? tlItems[tlIdx].contentIndex,
      toolCallIndex: event.toolCallIndex ?? tlItems[tlIdx].toolCallIndex,
    };
  }

  // Update projection
  proj = {
    ...proj,
    itemsById: { ...proj.itemsById },
  };
  if (tlItemId in proj.itemsById) {
    proj.itemsById[tlItemId] = {
      ...proj.itemsById[tlItemId],
      kind: "tool-result",
      toolStatus: event.isError ? "error" : "success",
      toolResult: event.result,
    };
  }

  return {
    ...state,
    transcript: updatedTranscript,
    timeline: {
      ...state.timeline,
      items: tlItems,
      collapsedToolCallIds: new Set([...state.timeline.collapsedToolCallIds, event.id]),
    },
    stream: {
      ...state.stream,
      currentToolCallId: undefined,
    },
    projection: proj,
  };
}
