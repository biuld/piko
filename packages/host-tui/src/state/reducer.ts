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
          layout: {
            ...state.layout,
            chat: {
              ...state.layout.chat,
              scrollAnchor: state.layout.chat.scrollAnchor === "manual" ? "manual" : "bottom",
            },
          },
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
        layout: {
          ...state.layout,
          chat: {
            ...state.layout.chat,
            scrollAnchor: state.layout.chat.scrollAnchor === "manual" ? "manual" : "bottom",
          },
        },
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
        layout: {
          ...state.layout,
          chat: { ...state.layout.chat, scrollAnchor: event.anchor },
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

    default: {
      const _exhaustive: never = event;
      return state;
    }
  }
}
