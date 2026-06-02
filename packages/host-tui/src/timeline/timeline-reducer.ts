// ============================================================================
// Timeline reducer — pure function for timeline state updates
// ============================================================================

import type { TimelineAnchor, TimelineItem, TuiTimelineState } from "./types.js";

export type TimelineAction =
  | { type: "add_item"; item: TimelineItem }
  | { type: "update_streaming"; text: string; streamingItemId: string }
  | { type: "finalize_streaming"; streamingItemId: string }
  | { type: "update_tool_result"; toolCallId: string; status: "success" | "error"; result: unknown }
  | { type: "set_anchor"; anchor: TimelineAnchor; itemId?: string }
  | { type: "user_scrolled"; atBottom: boolean }
  | { type: "jump_latest" }
  | { type: "toggle_expand"; itemId: string }
  | { type: "toggle_collapse_tool"; toolCallId: string }
  | { type: "new_content_added" }
  | { type: "reset" };

export function timelineReducer(state: TuiTimelineState, action: TimelineAction): TuiTimelineState {
  switch (action.type) {
    case "add_item": {
      const newState = { ...state, items: [...state.items, action.item] };
      // Auto-scroll
      if (state.anchor === "bottom") {
        newState.pendingNewItems = 0;
      } else {
        newState.pendingNewItems = state.pendingNewItems + 1;
      }
      return newState;
    }

    case "update_streaming": {
      const idx = state.items.findIndex((i) => i.id === action.streamingItemId);
      if (idx >= 0) {
        const items = [...state.items];
        items[idx] = { ...items[idx], text: action.text, isStreaming: true };
        return { ...state, items, streamingItemId: action.streamingItemId };
      }
      return state;
    }

    case "finalize_streaming": {
      const idx = state.items.findIndex((i) => i.id === action.streamingItemId);
      if (idx >= 0) {
        const items = [...state.items];
        items[idx] = {
          ...items[idx],
          kind: "assistant-message",
          isStreaming: false,
        };
        return { ...state, items, streamingItemId: undefined };
      }
      return state;
    }

    case "update_tool_result": {
      const idx = state.items.findIndex((i) => i.toolCallId === action.toolCallId);
      if (idx >= 0) {
        const items = [...state.items];
        items[idx] = {
          ...items[idx],
          toolStatus: action.status,
          toolResult: action.result,
        };
        return { ...state, items };
      }
      return state;
    }

    case "set_anchor": {
      return {
        ...state,
        anchor: action.anchor,
        anchorItemId: action.itemId,
        atBottom: action.anchor === "bottom",
      };
    }

    case "user_scrolled": {
      if (!action.atBottom && state.atBottom) {
        return {
          ...state,
          userScrolled: true,
          anchor: "manual",
          atBottom: false,
        };
      }
      return { ...state, atBottom: action.atBottom };
    }

    case "jump_latest": {
      return {
        ...state,
        anchor: "bottom",
        atBottom: true,
        userScrolled: false,
        pendingNewItems: 0,
        anchorItemId: undefined,
      };
    }

    case "toggle_expand": {
      const expanded = new Set(state.expandedItemIds);
      if (expanded.has(action.itemId)) {
        expanded.delete(action.itemId);
      } else {
        expanded.add(action.itemId);
      }
      return { ...state, expandedItemIds: expanded };
    }

    case "toggle_collapse_tool": {
      const collapsed = new Set(state.collapsedToolCallIds);
      if (collapsed.has(action.toolCallId)) {
        collapsed.delete(action.toolCallId);
      } else {
        collapsed.add(action.toolCallId);
      }
      return { ...state, collapsedToolCallIds: collapsed };
    }

    case "new_content_added": {
      if (state.anchor === "bottom" && state.atBottom) {
        return { ...state, pendingNewItems: 0 };
      }
      return { ...state, pendingNewItems: state.pendingNewItems + 1 };
    }

    case "reset": {
      return {
        items: [],
        anchor: "bottom",
        atBottom: true,
        userScrolled: false,
        pendingNewItems: 0,
        expandedItemIds: new Set(),
        collapsedToolCallIds: new Set(),
      };
    }

    default:
      return state;
  }
}
