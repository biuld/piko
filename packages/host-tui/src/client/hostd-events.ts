// ============================================================================
// hostd-events — HostEvent → TuiEvent mapping
// ============================================================================

import type { TuiEvent } from "../state/events.js";
import { entriesToTranscript } from "../timeline/entries-to-transcript.js";
import type { HostEvent } from "./hostd-protocol.js";

/**
 * Map a unified HostEvent to TUI event(s).
 * Returns null when the event is consumed internally (transcript, etc.).
 */
export function hostEventToTuiEvents(event: HostEvent): TuiEvent | TuiEvent[] | null {
  switch (event.type) {
    // Turn lifecycle
    case "turn_started":
      return { type: "stream_started" };
    case "turn_completed":
    case "turn_cancelled":
      return { type: "stream_settled" };
    case "turn_failed":
      return [{ type: "stream_settled" }, { type: "turn_failed", error: event.error }];

    // Agent panel state is projected from state snapshots. Do not dispatch
    // lifecycle events unless TuiState owns an agents slice.
    case "task_started":
    case "task_completed":
      return null;
    case "task_transcript_committed":
      return {
        type: "task_transcript_committed",
        taskId: event.task_id,
        parentTaskId: event.parent_task_id,
        messages: event.messages,
      } as Extract<TuiEvent, { type: "task_transcript_committed" }>;

    // Streaming
    case "text_delta":
      return { type: "assistant_delta", delta: event.delta };
    case "thinking_delta":
      return { type: "thinking_delta", delta: event.delta };
    case "tool_start":
      return {
        type: "tool_call_started",
        id: event.tool_call_id,
        name: event.tool_name,
        args: event.args,
      };
    case "tool_end":
      return {
        type: "tool_call_ended",
        id: event.tool_call_id,
        name: event.tool_name,
        result: event.result,
        isError: event.is_error,
      };

    // Approval
    case "approval_requested":
      return {
        type: "approval_needed",
        toolEntityId: event.approval_id,
        callId: event.approval_id,
        toolName: event.tool_name,
        toolArgs: event.tool_args,
      };
    case "approval_resolved":
      return {
        type: "approval_resolved",
        toolEntityId: event.approval_id,
        callId: event.approval_id,
        decision: event.decision,
      };

    // Queue
    case "queue_update":
      return {
        type: "queue_update",
        steerCount: event.steer_count,
        followUpCount: event.follow_up_count,
      };

    // Session
    case "session_created":
      return { type: "session_info_updated", sessionId: event.session_id };
    case "state_snapshot":
    case "session_opened":
      return [
        {
          type: "session_resumed",
          sessionId: event.session_id,
          sessionName: event.snapshot.name,
          transcript: entriesToTranscript(event.snapshot.entries),
        },
        {
          type: "session_info_updated",
          sessionId: event.session_id,
          sessionName: event.snapshot.name,
          messageCount: event.snapshot.entries.length,
        },
      ];
    case "session_listed":
      return null;

    // Model config — consumed by session state, no direct TUI event yet
    case "model_config_changed":
      return null;

    // Domain messages — consumed by transcript reducer, not TUI consumer
    case "user_message_submitted":
    case "assistant_message_completed":
    case "tool_result_committed":
      return null;

    // Message boundaries — optional
    case "message_start":
    case "message_end":
      return null;

    // Unhandled domain events
    default:
      return null;
  }
}
