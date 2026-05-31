import type { EngineEvent } from "piko-engine-protocol";
import type { HostLifecycleEvent } from "../host/lifecycle-events.js";

/**
 * Mutable state used by the engine event processor to track
 * message boundaries across a single step's event stream.
 */
export interface EngineEventState {
  currentMessageId: string | null;
  toolNameById: Map<string, string>;
}

/**
 * Create an engine event processor. Returns a state object (reset per step)
 * and a process function that maps raw engine events to host lifecycle events.
 *
 * Guarantees:
 * - Every message_end is preceded by a matching message_start.
 * - All events within a single step share the same messageId (no duplicate starts).
 * - Tool-only responses (no message_delta) correctly emit message_start before tool calls.
 */
export function createEngineEventProcessor(
  onEvent: ((event: EngineEvent) => void) | undefined,
  emitLifecycle: (event: HostLifecycleEvent) => void,
): {
  state: EngineEventState;
  process: (event: EngineEvent) => void;
} {
  const state: EngineEventState = {
    currentMessageId: null,
    toolNameById: new Map(),
  };

  /** Get or create the step-level message ID. */
  function stepMessageId(): string {
    if (!state.currentMessageId) {
      state.currentMessageId = `msg-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      emitLifecycle({
        type: "message_start",
        messageId: state.currentMessageId,
        role: "assistant",
      });
    }
    return state.currentMessageId;
  }

  function process(event: EngineEvent): void {
    // Always forward raw engine events to the consumer (TUI, etc.)
    onEvent?.(event);

    switch (event.type) {
      case "message_delta": {
        const msgId = event.messageId;
        if (!state.currentMessageId || state.currentMessageId !== msgId) {
          state.currentMessageId = msgId;
          emitLifecycle({ type: "message_start", messageId: msgId, role: "assistant" });
        }
        emitLifecycle({
          type: "message_update",
          messageId: msgId,
          delta: event.delta,
          isThinking: false,
        });
        break;
      }
      case "thinking_delta": {
        const msgId = event.messageId;
        if (!state.currentMessageId || state.currentMessageId !== msgId) {
          state.currentMessageId = msgId;
          emitLifecycle({ type: "message_start", messageId: msgId, role: "assistant" });
        }
        emitLifecycle({
          type: "message_update",
          messageId: msgId,
          delta: event.delta,
          isThinking: true,
        });
        break;
      }
      case "message_end": {
        stepMessageId();
        const msgContent =
          typeof event.message.content === "string"
            ? event.message.content
            : Array.isArray(event.message.content)
              ? event.message.content
                  .filter((c): c is { type: "text"; text: string } => c.type === "text")
                  .map((c) => c.text)
                  .join("")
              : "";
        emitLifecycle({
          type: "message_end",
          message: { role: "assistant", content: msgContent },
        });
        state.currentMessageId = null;
        break;
      }
      case "tool_call_start":
        stepMessageId();
        state.toolNameById.set(event.id, event.name);
        emitLifecycle({
          type: "tool_execution_start",
          toolCallId: event.id,
          toolName: event.name,
          args: event.args,
        });
        break;
      case "tool_call_end":
        emitLifecycle({
          type: "tool_execution_end",
          toolCallId: event.id,
          toolName: state.toolNameById.get(event.id) ?? event.id,
          result: event.result,
          isError: event.isError,
        });
        break;
      // step_start, step_end, approval_requested, error — not mapped to host lifecycle
    }
  }

  return { state, process };
}
