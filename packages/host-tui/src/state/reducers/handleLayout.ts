// ============================================================================
// Layout reducers — layout_resized, region_focused, chat_scrolled, tool_block_toggled
// ============================================================================

import type {
  ChatScrolledEvent,
  LayoutResizedEvent,
  RegionFocusedEvent,
  ToolBlockToggledEvent,
} from "../events.js";
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

export function handleRegionFocused(state: TuiState, event: RegionFocusedEvent): TuiState {
  return {
    ...state,
    layout: { ...state.layout, activeRegion: event.region },
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
    layout: {
      ...state.layout,
      chat: { ...state.layout.chat },
    },
  };
}

export function handleToolBlockToggled(state: TuiState, event: ToolBlockToggledEvent): TuiState {
  const newCollapsed = new Set(state.layout.chat.collapsedToolCallIds);
  if (newCollapsed.has(event.toolCallId)) {
    newCollapsed.delete(event.toolCallId);
  } else {
    newCollapsed.add(event.toolCallId);
  }
  return {
    ...state,
    layout: {
      ...state.layout,
      chat: {
        ...state.layout.chat,
        collapsedToolCallIds: newCollapsed,
      },
    },
  };
}
