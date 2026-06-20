import { buildTimelineItem } from "../../timeline/timeline-builder.js";
import type {
  EditorDraftChangedEvent,
  EditorDraftReplacedEvent,
  UserSubmittedEvent,
} from "../events.js";
import type { TuiMessageViewModel, TuiState } from "../state.js";
import { nextMessageId, pushTimelineItem } from "./helpers.js";

export function handleUserSubmitted(state: TuiState, event: UserSubmittedEvent): TuiState {
  const userMsg: TuiMessageViewModel = {
    id: nextMessageId(),
    role: "user",
    text: event.text,
  };
  const timelineItem = buildTimelineItem(userMsg);
  const tl = pushTimelineItem(state.timeline.items, timelineItem, state.timeline.anchor);
  return {
    ...state,
    input: state.input,
    transcript: [...state.transcript, userMsg],
    timeline: {
      ...state.timeline,
      items: tl.items,
      pendingNewItems: state.timeline.pendingNewItems + tl.pendingDelta,
    },
    stream: {
      ...state.stream,
      status: "running",
      assistantText: "",
      thinkingActive: false,
      currentToolCallId: undefined,
      queue: undefined,
    },
  };
}

export function handleEditorDraftChanged(
  state: TuiState,
  event: EditorDraftChangedEvent,
): TuiState {
  return {
    ...state,
    input: {
      ...state.input,
      draft: event.text,
    },
  };
}

export function handleEditorDraftReplaced(
  state: TuiState,
  event: EditorDraftReplacedEvent,
): TuiState {
  return {
    ...state,
    input: {
      ...state.input,
      draft: event.text,
      revision: state.input.revision + 1,
    },
  };
}
