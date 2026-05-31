import type { HostLifecycleEvent } from "../host/lifecycle-events.js";
import type { SessionState } from "../session/index.js";
import type { FollowUpMessage, NextTurnMessage, SteeringMessage } from "./types.js";

// ============================================================================
// Lifecycle emitter factory
// ============================================================================

/**
 * Create a safe lifecycle emitter that swallows errors from the callback.
 * Returns a function suitable for use as the `emitLifecycle` closure.
 */
export function createLifecycleEmitter(
  onLifecycleEvent: ((event: HostLifecycleEvent) => void) | undefined,
): (event: HostLifecycleEvent) => void {
  return (event: HostLifecycleEvent) => {
    try {
      const result = onLifecycleEvent?.(event);
      if (result && typeof (result as unknown as Promise<void>).catch === "function") {
        (result as unknown as Promise<void>).catch(() => {});
      }
    } catch {
      // Swallow sync errors from lifecycle callbacks
    }
  };
}

// ============================================================================
// Message lifecycle helpers
// ============================================================================

/** Emit lifecycle for a synthetic assistant failure message. */
export function emitFailureMessage(
  emitLifecycle: (event: HostLifecycleEvent) => void,
  errorText: string,
): void {
  const failureId = `failure-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
  emitLifecycle({ type: "message_start", messageId: failureId, role: "assistant" });
  emitLifecycle({
    type: "message_end",
    message: { role: "assistant", content: errorText, messageId: failureId },
  });
}

/** Emit lifecycle for a user message being injected (steering / follow-up / next-turn). */
export function emitUserMessageLifecycle(
  emitLifecycle: (event: HostLifecycleEvent) => void,
  text: string,
  source: string,
): void {
  const msgId = `user-${source}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
  emitLifecycle({ type: "message_start", messageId: msgId, role: "user" });
  emitLifecycle({
    type: "message_end",
    message: { role: "user", content: text, messageId: msgId },
  });
}

// ============================================================================
// Queue lifecycle
// ============================================================================

/** Emit queue_update with current queue sizes and optional message previews. */
export function emitQueueUpdate(
  emitLifecycle: (event: HostLifecycleEvent) => void,
  steeringQueue: SteeringMessage[] | undefined,
  followUpQueue: FollowUpMessage[] | undefined,
  nextTurnQueue: NextTurnMessage[] | undefined,
): void {
  const MAX_PREVIEW_LEN = 80;
  const steerPreview = steeringQueue?.[0]?.text?.slice(0, MAX_PREVIEW_LEN);
  const followUpPreview = followUpQueue?.[0]?.text?.slice(0, MAX_PREVIEW_LEN);
  const nextTurnPreview = nextTurnQueue?.[0]?.text?.slice(0, MAX_PREVIEW_LEN);
  emitLifecycle({
    type: "queue_update",
    steerCount: steeringQueue?.length ?? 0,
    followUpCount: followUpQueue?.length ?? 0,
    nextTurnCount: nextTurnQueue?.length ?? 0,
    steerPreview,
    followUpPreview,
    nextTurnPreview,
  });
}

// ============================================================================
// Save point
// ============================================================================

/**
 * Emit save_point lifecycle event and flush pending session writes.
 * Called at turn boundaries so messages are persisted incrementally.
 */
export async function emitSavePoint(
  emitLifecycle: (event: HostLifecycleEvent) => void,
  onSavePoint: ((session: SessionState) => void | Promise<void>) | undefined,
  currentSession: SessionState,
): Promise<void> {
  emitLifecycle({ type: "save_point", hadPendingWrites: true });
  if (onSavePoint) {
    await onSavePoint(currentSession);
  }
}
