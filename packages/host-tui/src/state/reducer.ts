// ============================================================================
// TUI Reducer — pure function: (state, event) → state
// ============================================================================

import type { TuiEvent } from "./events.js";
import type { TuiMessageViewModel, TuiState } from "./state.js";

// ============================================================================
// Helpers
// ============================================================================

let messageIdCounter = 0;
function nextMessageId(): string {
  return `msg-${++messageIdCounter}`;
}

function findLastAssistantIndex(transcript: TuiMessageViewModel[]): number {
  for (let i = transcript.length - 1; i >= 0; i--) {
    if (transcript[i].role === "assistant") return i;
  }
  return -1;
}

function findToolCallIndex(transcript: TuiMessageViewModel[], toolCallId: string): number {
  for (let i = transcript.length - 1; i >= 0; i--) {
    const msg = transcript[i];
    if (msg.toolBlock?.toolCallId === toolCallId) return i;
  }
  return -1;
}

// ============================================================================
// Reducer
// ============================================================================

export function tuiReducer(state: TuiState, event: TuiEvent): TuiState {
  switch (event.type) {
    // ---- Input ----
    case "user_input_changed": {
      return {
        ...state,
        input: { ...state.input, text: event.text },
      };
    }

    case "user_submitted": {
      return {
        ...state,
        input: { ...state.input, text: "" },
        transcript: [
          ...state.transcript,
          {
            id: nextMessageId(),
            role: "user",
            text: event.text,
          },
        ],
        stream: {
          ...state.stream,
          status: "running",
          assistantText: "",
          thinkingActive: false,
          currentToolCallId: undefined,
          currentToolName: undefined,
          queueInfo: undefined,
        },
      };
    }

    // ---- Stream lifecycle ----
    case "stream_started": {
      return {
        ...state,
        stream: {
          ...state.stream,
          status: "running",
          assistantText: "",
          thinkingActive: false,
        },
      };
    }

    case "assistant_delta": {
      const lastIdx = findLastAssistantIndex(state.transcript);
      const text = state.stream.assistantText + event.delta;

      if (lastIdx >= 0) {
        const updated = [...state.transcript];
        updated[lastIdx] = {
          ...updated[lastIdx],
          text,
          isStreaming: true,
        };
        return {
          ...state,
          transcript: updated,
          stream: { ...state.stream, assistantText: text },
        };
      }

      // No assistant message yet — create one
      return {
        ...state,
        transcript: [
          ...state.transcript,
          {
            id: nextMessageId(),
            role: "assistant",
            text,
            isStreaming: true,
          },
        ],
        stream: { ...state.stream, assistantText: text },
      };
    }

    case "thinking_delta": {
      return {
        ...state,
        stream: { ...state.stream, thinkingActive: true },
      };
    }

    // ---- Tool calls ----
    case "tool_call_started": {
      const wasManual = state.timeline.anchor === "manual";
      return {
        ...state,
        transcript: [
          ...state.transcript,
          {
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
          },
        ],
        stream: {
          ...state.stream,
          currentToolCallId: event.id,
          currentToolName: event.name,
        },
        timeline: wasManual
          ? { ...state.timeline, pendingNewItems: state.timeline.pendingNewItems + 1 }
          : state.timeline,
      };
    }

    case "tool_call_ended": {
      const toolIdx = findToolCallIndex(state.transcript, event.id);
      if (toolIdx >= 0) {
        const updated = [...state.transcript];
        updated[toolIdx] = {
          ...updated[toolIdx],
          toolBlock: {
            ...updated[toolIdx].toolBlock!,
            status: event.isError ? "error" : "success",
            result: event.result,
          },
        };
        return {
          ...state,
          transcript: updated,
          stream: {
            ...state.stream,
            currentToolCallId: undefined,
            currentToolName: undefined,
          },
        };
      }
      return state;
    }

    // ---- Turn completion ----
    case "turn_finished": {
      // Rebuild the canonical transcript from engine result messages.
      // This replaces the streaming-incremental transcript with the authoritative
      // multi-step message list from the engine, ensuring correct ordering.
      const canonicalMessages: TuiMessageViewModel[] = [];
      for (const msg of event.transcript) {
        if (msg.role === "user") {
          canonicalMessages.push({
            id: nextMessageId(),
            role: "user",
            text: typeof msg.content === "string" ? msg.content : "",
          });
        } else if (msg.role === "assistant") {
          const content = (msg as any).content;
          if (Array.isArray(content)) {
            // Extract text blocks
            const textBlocks = content.filter((b: any) => b.type === "text");
            const text = textBlocks.map((b: any) => b.text).join("\n");
            if (text) {
              canonicalMessages.push({
                id: nextMessageId(),
                role: "assistant",
                text,
              });
            }
            // Extract tool calls
            for (const block of content) {
              if (block.type === "toolCall") {
                canonicalMessages.push({
                  id: nextMessageId(),
                  role: "tool",
                  text: "",
                  toolBlock: {
                    toolCallId: block.id ?? `tc-${canonicalMessages.length}`,
                    name: block.name ?? "unknown",
                    args: block.arguments ?? block.args ?? {},
                    status: "success",
                    isCollapsed: false,
                  },
                });
              }
            }
          }
        } else if (msg.role === "toolResult" || (msg as any).role === "tool") {
          const anyMsg = msg as any;
          canonicalMessages.push({
            id: nextMessageId(),
            role: "tool",
            text: "",
            toolBlock: {
              toolCallId: anyMsg.toolCallId ?? `tc-${canonicalMessages.length}`,
              name: anyMsg.toolName ?? "tool",
              args: {},
              status: anyMsg.isError ? "error" : "success",
              result: anyMsg.content ?? anyMsg.details,
              isCollapsed: false,
            },
          });
        }
      }

      const statusMsg =
        event.status === "max_steps"
          ? "Stopped after reaching max steps"
          : event.status === "aborted"
            ? "Interrupted"
            : event.status === "error"
              ? "Run failed"
              : undefined;

      return {
        ...state,
        transcript: canonicalMessages.length > 0 ? canonicalMessages : state.transcript,
        stream: {
          ...state.stream,
          status: statusMsg ? "idle" : "idle",
          assistantText: "",
          thinkingActive: false,
          currentToolCallId: undefined,
          currentToolName: undefined,
          queueInfo: undefined,
        },
      };
    }

    case "turn_failed": {
      return {
        ...state,
        transcript: [
          ...state.transcript,
          {
            id: nextMessageId(),
            role: "assistant",
            text: `Error: ${event.error}`,
          },
        ],
        stream: {
          ...state.stream,
          status: "idle",
          assistantText: "",
          thinkingActive: false,
          currentToolCallId: undefined,
          currentToolName: undefined,
        },
      };
    }

    case "aborted": {
      return {
        ...state,
        stream: { ...state.stream, status: "aborting" },
      };
    }

    // ---- Queue updates (steer / follow-up) ----
    case "queue_update": {
      const parts: string[] = [];
      if (event.steerCount > 0) {
        parts.push(
          `Steer:${event.steerCount}${event.steerPreview ? ` "${event.steerPreview}"` : ""}`,
        );
      }
      if (event.followUpCount > 0) {
        parts.push(
          `FollowUp:${event.followUpCount}${event.followUpPreview ? ` "${event.followUpPreview}"` : ""}`,
        );
      }
      return {
        ...state,
        stream: {
          ...state.stream,
          queueInfo: parts.length > 0 ? parts.join(" │ ") : undefined,
        },
      };
    }

    // ---- Overlays ----
    case "overlay_opened": {
      return {
        ...state,
        overlay: event.overlay,
        layout: {
          ...state.layout,
          activeRegion: "overlay",
          overlay: {
            kind: event.overlay.kind,
            placement: event.overlay.placement,
          },
        },
      };
    }

    case "overlay_closed": {
      return {
        ...state,
        overlay: null,
        layout: {
          ...state.layout,
          activeRegion: "editor",
          overlay: undefined,
        },
      };
    }

    // ---- Layout ----
    case "layout_resized": {
      return {
        ...state,
        layout: {
          ...state.layout,
          viewport: { width: event.width, height: event.height },
        },
        // Note: mode recalculation happens in selectors/policies
      };
    }

    case "region_focused": {
      return {
        ...state,
        layout: { ...state.layout, activeRegion: event.region },
      };
    }

    case "chat_scrolled": {
      return {
        ...state,
        timeline: {
          ...state.timeline,
          anchor: event.anchor === "bottom" ? "bottom" : "manual",
          atBottom: event.anchor === "bottom",
          userScrolled: event.anchor !== "bottom",
        },
        layout: {
          ...state.layout,
          chat: { ...state.layout.chat },
        },
      };
    }

    case "tool_block_toggled": {
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

    // ---- Model ----
    case "model_changed": {
      return {
        ...state,
        model: {
          ...state.model,
          current: event.model,
          providerConfig: event.providerConfig,
        },
      };
    }

    case "thinking_level_changed": {
      return {
        ...state,
        model: { ...state.model, thinkingLevel: event.level },
      };
    }

    // ---- Session ----
    case "session_resumed": {
      return {
        ...state,
        session: {
          ...state.session,
          sessionId: event.sessionId,
          sessionName: event.sessionName ?? state.session.sessionName,
          messageCount: event.transcript.length,
        },
        transcript: event.transcript,
        stream: { ...state.stream, status: "idle" },
      };
    }

    case "session_forked": {
      return {
        ...state,
        session: {
          ...state.session,
          sessionId: event.sessionId,
        },
      };
    }

    case "session_info_updated": {
      return {
        ...state,
        session: {
          ...state.session,
          sessionId: event.sessionId ?? state.session.sessionId,
          sessionName: event.sessionName ?? state.session.sessionName,
          messageCount: event.messageCount ?? state.session.messageCount,
        },
      };
    }

    // ---- Usage ----
    case "usage_updated": {
      return {
        ...state,
        usage: {
          inputTokens: event.inputTokens ?? state.usage.inputTokens,
          outputTokens: event.outputTokens ?? state.usage.outputTokens,
          cacheReadTokens: event.cacheReadTokens ?? state.usage.cacheReadTokens,
          cacheWriteTokens: event.cacheWriteTokens ?? state.usage.cacheWriteTokens,
          totalCost: event.totalCost ?? state.usage.totalCost,
          contextWindow: event.contextWindow ?? state.usage.contextWindow,
          contextPercent: event.contextPercent ?? state.usage.contextPercent,
        },
      };
    }

    // ---- Extensions ----
    case "extension_status_set": {
      const newSlots = new Map(state.extensions.statusSlots);
      if (event.text === undefined) {
        newSlots.delete(event.key);
      } else {
        newSlots.set(event.key, event.text);
      }
      return {
        ...state,
        extensions: {
          ...state.extensions,
          statusSlots: newSlots,
        },
      };
    }

    // ---- Notifications ----
    case "notification_added": {
      const notifs = [event.notification, ...state.notifications].slice(0, 200);
      return {
        ...state,
        notifications: notifs,
      };
    }

    case "notification_cleared": {
      if (event.id) {
        return {
          ...state,
          notifications: state.notifications.filter((n) => n.id !== event.id),
        };
      }
      return { ...state, notifications: [] };
    }

    case "notification_read": {
      const updatedNotifs = state.notifications.map((n) => {
        if (!event.id || n.id === event.id) {
          return { ...n, readAt: n.readAt ?? Date.now() };
        }
        return n;
      });
      return { ...state, notifications: updatedNotifs };
    }

    // ---- Surfaces ----
    case "surface_opened": {
      return {
        ...state,
        surfaces: [...state.surfaces, event.surface],
      };
    }

    case "surface_closed": {
      const closedId = event.surfaceId;
      // Close the surface and all descendants
      const closedIds = new Set<string>([closedId]);
      for (const s of state.surfaces) {
        if (s.parentId && closedIds.has(s.parentId)) {
          closedIds.add(s.id);
        }
      }
      return {
        ...state,
        surfaces: state.surfaces.filter((s) => !closedIds.has(s.id)),
        layout: {
          ...state.layout,
          activeRegion:
            state.surfaces.filter((s) => !closedIds.has(s.id)).length === 0
              ? "editor"
              : state.layout.activeRegion,
        },
      };
    }

    // ---- Timeline ----
    case "timeline_scrolled": {
      return {
        ...state,
        timeline: {
          ...state.timeline,
          anchor: event.anchor,
          atBottom: event.atBottom,
          userScrolled: event.anchor === "manual",
        },
      };
    }

    case "timeline_item_toggled": {
      const newExpanded = new Set(state.timeline.expandedItemIds);
      if (newExpanded.has(event.itemId)) {
        newExpanded.delete(event.itemId);
      } else {
        newExpanded.add(event.itemId);
      }
      return {
        ...state,
        timeline: { ...state.timeline, expandedItemIds: newExpanded },
      };
    }

    case "timeline_tool_toggled": {
      const newCollapsed = new Set(state.timeline.collapsedToolCallIds);
      if (newCollapsed.has(event.toolCallId)) {
        newCollapsed.delete(event.toolCallId);
      } else {
        newCollapsed.add(event.toolCallId);
      }
      return {
        ...state,
        timeline: { ...state.timeline, collapsedToolCallIds: newCollapsed },
      };
    }

    case "timeline_pending_update": {
      return {
        ...state,
        timeline: { ...state.timeline, pendingNewItems: event.pendingNewItems },
      };
    }

    // ---- Focus ----
    case "focus_changed": {
      return {
        ...state,
        focus: {
          ...state.focus,
          activeOwnerId: event.activeOwnerId,
          region: event.region,
        },
      };
    }

    // ---- Autocomplete ----
    case "autocomplete_active": {
      return {
        ...state,
        autocomplete: event.active
          ? { active: true, selectedIndex: event.selectedIndex ?? 0, acceptToken: 0 }
          : undefined,
      };
    }

    case "autocomplete_navigate": {
      if (!state.autocomplete?.active) return state;
      return {
        ...state,
        autocomplete: {
          ...state.autocomplete,
          selectedIndex: Math.max(0, state.autocomplete.selectedIndex + event.delta),
        },
      };
    }

    case "autocomplete_accept": {
      if (!state.autocomplete?.active) return state;
      return {
        ...state,
        autocomplete: {
          ...state.autocomplete,
          acceptToken: state.autocomplete.acceptToken + 1,
        },
      };
    }

    default: {
      const _exhaustive: never = event;
      return state;
    }
  }
}
