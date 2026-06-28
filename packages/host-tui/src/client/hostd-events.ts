// ============================================================================
// hostd-events — HostEvent → TuiEvent mapping
// ============================================================================

import type { Usage } from "../shared/types.js";
import type { TuiEvent } from "../state/events.js";
import { entriesToTranscript } from "../timeline/entries-to-transcript.js";
import type { HostEvent } from "./hostd-protocol.js";

/**
 * Map a unified HostEvent to TUI event(s).
 * Returns null when the event is consumed internally (transcript, etc.).
 */
export function hostEventToTuiEvents(event: HostEvent): TuiEvent | TuiEvent[] | null {
  switch (event.type) {
    case "auth_login_device_code":
    case "auth_login_success":
    case "auth_login_failed":
    case "auth_logged_out":
      return event as TuiEvent;

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
    case "session_opened": {
      const snapshotUsage: Usage | undefined = event.snapshot.cumulativeUsage as Usage | undefined;
      return [
        {
          type: "session_resumed",
          sessionId: event.session_id,
          sessionName: event.snapshot.name,
          transcript: entriesToTranscript(event.snapshot.entries),
          entries: event.snapshot.entries,
          currentLeafId: event.snapshot.current_leaf_id ?? null,
          cumulativeUsage: snapshotUsage,
        },
        {
          type: "session_info_updated",
          sessionId: event.session_id,
          sessionName: event.snapshot.name,
          messageCount: event.snapshot.entries.length,
        },
      ];
    }
    case "session_listed":
      return null;

    // Model config — broadcast to update current model + thinking level
    case "model_config_changed": {
      const events: TuiEvent[] = [];
      if (event.model_id && event.provider) {
        events.push({
          type: "model_changed",
          model: {
            id: event.model_id,
            name: event.model_id,
            provider: event.provider,
          } as any,
          providerConfig: {} as any,
        });
      }
      if (event.thinkingLevel) {
        events.push({
          type: "thinking_level_changed",
          level: event.thinkingLevel,
        });
      }
      return events.length > 0 ? events : null;
    }

    // Domain messages — extract usage from completed assistant messages
    case "assistant_message_completed":
      if (event.message.role === "assistant" && event.message.usage) {
        return {
          type: "usage_accrued",
          inputTokens: event.message.usage.input,
          outputTokens: event.message.usage.output,
          cacheReadTokens: (event.message.usage as any).cache_read ?? (event.message.usage as any).cacheRead ?? 0,
          cacheWriteTokens: (event.message.usage as any).cache_write ?? (event.message.usage as any).cacheWrite ?? 0,
          totalCost: event.message.usage.cost.total,
        };
      }
      return null;
    case "user_message_submitted":
    case "tool_result_committed":
      return null;

    case "model_listed":
      return {
        type: "model_list_received",
        providers: event.providers,
      };

    // Message boundaries — optional
    case "message_start":
    case "message_end":
      return null;

    // Unhandled domain events
    default:
      return null;
  }
}
