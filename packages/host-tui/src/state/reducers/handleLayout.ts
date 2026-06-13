// ============================================================================
// Layout reducers — layout_resized, chat_scrolled
// ============================================================================

import type { ChatScrolledEvent, LayoutResizedEvent } from "../events.js";
import type { TuiState } from "../state.js";

export function handleLayoutResized(state: TuiState, event: LayoutResizedEvent): TuiState {
  const vp = state.layout.viewport;
  if (vp.width === event.width && vp.height === event.height) return state;
  return {
    ...state,
    layout: {
      ...state.layout,
      viewport: { width: event.width, height: event.height },
    },
  };
}

export function handleChatScrolled(state: TuiState, event: ChatScrolledEvent): TuiState {
  const targetAnchor = event.anchor === "bottom" ? "bottom" : "manual";
  const targetAtBottom = event.anchor === "bottom";
  const tl = state.timeline;
  if (
    tl.anchor === targetAnchor &&
    tl.atBottom === targetAtBottom &&
    tl.userScrolled === (event.anchor !== "bottom")
  ) {
    return state;
  }
  return {
    ...state,
    timeline: {
      ...tl,
      anchor: targetAnchor,
      atBottom: targetAtBottom,
      userScrolled: event.anchor !== "bottom",
      pendingNewItems: targetAtBottom ? 0 : tl.pendingNewItems,
    },
  };
}
