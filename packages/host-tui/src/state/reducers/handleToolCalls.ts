// ============================================================================
// Tool lifecycle reducers.
//
// start/end are idempotent upserts. Either event may be the first one observed;
// tools without a parent are rendered provisionally and re-parented when the
// assistant message arrives.
// ============================================================================

import { materializeProjection, upsertToolItem } from "../../timeline/projection.js";
import { buildTimelineItem } from "../../timeline/timeline-builder.js";
import type { ToolCallEndedEvent, ToolCallStartedEvent } from "../events.js";
import type { ToolBlockViewModel, TuiMessageViewModel, TuiState } from "../state.js";
import { applySeq } from "./diagnostics.js";
import { findToolCallIndex, findToolEntityIndex, nextMessageId } from "./helpers.js";

export function handleToolCallStarted(state: TuiState, event: ToolCallStartedEvent): TuiState {
  return upsertToolLifecycle(state, event, {
    name: event.name,
    args: event.args,
    status: "running",
    isCollapsed: false,
  });
}

export function handleToolCallEnded(state: TuiState, event: ToolCallEndedEvent): TuiState {
  const existing = findExistingToolBlock(state, event.id, event.entityId);
  const next = upsertToolLifecycle(state, event, {
    name: event.name || existing?.name || "tool",
    args: existing?.args ?? {},
    status: event.isError ? "error" : "success",
    result: event.result,
    isCollapsed: existing?.isCollapsed ?? false,
  });

  return {
    ...next,
    timeline: {
      ...next.timeline,
      collapsedToolCallIds: new Set([
        ...next.timeline.collapsedToolCallIds,
        event.entityId ?? event.id,
      ]),
    },
    stream: { ...next.stream, currentToolCallId: undefined },
  };
}

/** Approval is an independent delivery path; make its tool visible even if tool_start was missed. */
export function ensureToolForApproval(
  state: TuiState,
  event: { callId: string; toolEntityId?: string; toolName: string; toolArgs: unknown },
): TuiState {
  const entityId = event.toolEntityId ?? event.callId;
  if (`tool:${entityId}` in state.projection.itemsById) return state;
  return upsertToolLifecycle(
    state,
    {
      type: "tool_call_started",
      id: event.callId,
      entityId,
      name: event.toolName,
      args: event.toolArgs,
      parentMessageId: "",
      contentIndex: 0,
      toolCallIndex: 0,
    },
    {
      name: event.toolName,
      args: event.toolArgs,
      status: "pending",
      isCollapsed: false,
    },
  );
}

type ToolLifecycleEvent = ToolCallStartedEvent | ToolCallEndedEvent;

function upsertToolLifecycle(
  state: TuiState,
  event: ToolLifecycleEvent,
  toolBlock: Omit<ToolBlockViewModel, "toolCallId" | "toolEntityId">,
): TuiState {
  const runId = event.runId ?? `tool_${event.id}`;
  const entityId = event.entityId ?? event.id;
  let projection = applySeq(state.projection, runId, event.eventSeq);
  const entityIdx = findToolEntityIndex(state.transcript, entityId);
  const existingIdx = event.entityId
    ? entityIdx
    : entityIdx >= 0
      ? entityIdx
      : findToolCallIndex(state.transcript, event.id);
  const existingMessage = existingIdx >= 0 ? state.transcript[existingIdx] : undefined;
  const toolMessage: TuiMessageViewModel = {
    id: existingMessage?.id ?? nextMessageId(),
    role: "tool",
    text: existingMessage?.text ?? "",
    toolBlock: { ...toolBlock, toolEntityId: entityId, toolCallId: event.id },
  };
  const timelineItem = {
    ...buildTimelineItem(toolMessage),
    id: `tool:${entityId}`,
    toolEntityId: entityId,
    parentMessageId: event.parentMessageId ?? "",
    contentIndex: event.contentIndex ?? 0,
    toolCallIndex: event.toolCallIndex ?? 0,
    turnIndex: event.turnIndex,
    eventSeq: event.eventSeq,
    runId,
  };

  projection = upsertToolItem(
    projection,
    timelineItem,
    timelineItem.parentMessageId,
    timelineItem.toolCallIndex,
  );

  const transcript = [...state.transcript];
  if (existingIdx >= 0) transcript[existingIdx] = toolMessage;
  else transcript.push(toolMessage);

  const isNew = !state.projection.itemsById[timelineItem.id];
  return {
    ...state,
    transcript,
    timeline: {
      ...state.timeline,
      items: materializeProjection(projection),
      pendingNewItems:
        state.timeline.anchor === "manual" && isNew
          ? state.timeline.pendingNewItems + 1
          : state.timeline.pendingNewItems,
    },
    stream: { ...state.stream, currentToolCallId: event.id },
    projection,
  };
}

function findExistingToolBlock(
  state: TuiState,
  toolCallId: string,
  toolEntityId?: string,
): ToolBlockViewModel | undefined {
  const entityIndex = toolEntityId ? findToolEntityIndex(state.transcript, toolEntityId) : -1;
  const index = toolEntityId
    ? entityIndex
    : entityIndex >= 0
      ? entityIndex
      : findToolCallIndex(state.transcript, toolCallId);
  return index >= 0 ? state.transcript[index].toolBlock : undefined;
}
