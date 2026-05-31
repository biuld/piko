import type { HostLifecycleEvent } from "../host/lifecycle-events.js";
import type { SessionState } from "../session/index.js";
import { addUserMessage } from "../session/index.js";
import { emitQueueUpdate, emitUserMessageLifecycle } from "./lifecycle.js";
import type { FollowUpMessage, NextTurnMessage, QueueMode, SteeringMessage } from "./types.js";

// ============================================================================
// Steering drain
// ============================================================================

/**
 * Drain the steering queue into the session.
 * Returns the updated session and whether any messages were injected.
 */
export function drainSteering(
  emitLifecycle: (event: HostLifecycleEvent) => void,
  currentSession: SessionState,
  steeringQueue: SteeringMessage[] | undefined,
  followUpQueue: FollowUpMessage[] | undefined,
  nextTurnQueue: NextTurnMessage[] | undefined,
  steeringMode: QueueMode,
): { session: SessionState; drained: boolean } {
  if (!steeringQueue || steeringQueue.length === 0) {
    return { session: currentSession, drained: false };
  }
  const steered = steeringMode === "all" ? steeringQueue.splice(0) : steeringQueue.splice(0, 1);
  for (const s of steered) {
    currentSession = addUserMessage(currentSession, s.text, s.images);
    emitUserMessageLifecycle(emitLifecycle, s.text, "steer");
  }
  emitQueueUpdate(emitLifecycle, steeringQueue, followUpQueue, nextTurnQueue);
  return { session: currentSession, drained: true };
}

// ============================================================================
// Follow-up drain
// ============================================================================

/**
 * Drain the follow-up queue. Returns the updated session and the drained messages.
 */
export function drainFollowUp(
  emitLifecycle: (event: HostLifecycleEvent) => void,
  currentSession: SessionState,
  followUpQueue: FollowUpMessage[] | undefined,
  nextTurnQueue: NextTurnMessage[] | undefined,
  followUpMode: QueueMode,
): { session: SessionState; hasMore: boolean } {
  if (!followUpQueue || followUpQueue.length === 0) {
    return { session: currentSession, hasMore: false };
  }
  const drained = followUpMode === "all" ? followUpQueue.splice(0) : followUpQueue.splice(0, 1);
  for (const msg of drained) {
    currentSession = addUserMessage(currentSession, msg.text, msg.images);
    emitUserMessageLifecycle(emitLifecycle, msg.text, "follow-up");
  }
  emitQueueUpdate(emitLifecycle, undefined, followUpQueue, nextTurnQueue);
  return { session: currentSession, hasMore: followUpQueue.length > 0 };
}

// ============================================================================
// Next-turn drain
// ============================================================================

/**
 * Drain one message from the next-turn queue.
 * Returns the updated session and whether a message was consumed.
 */
export function drainNextTurn(
  emitLifecycle: (event: HostLifecycleEvent) => void,
  currentSession: SessionState,
  nextTurnQueue: NextTurnMessage[] | undefined,
  followUpQueue: FollowUpMessage[] | undefined,
): { session: SessionState; drained: boolean } {
  if (!nextTurnQueue || nextTurnQueue.length === 0) {
    return { session: currentSession, drained: false };
  }
  const nt = nextTurnQueue.splice(0, 1)[0]!;
  currentSession = addUserMessage(currentSession, nt.text, nt.images);
  emitUserMessageLifecycle(emitLifecycle, nt.text, "next-turn");
  emitQueueUpdate(emitLifecycle, undefined, followUpQueue, nextTurnQueue);
  return { session: currentSession, drained: true };
}
