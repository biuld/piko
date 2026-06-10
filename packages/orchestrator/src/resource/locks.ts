// ---- Lock manager ----

import type { LockMode, LockState } from "piko-orchestrator-protocol";
import type { OrchestratorCtx } from "../context.js";
import { emitToCtx } from "../context.js";

export function requestLock(
  ctx: OrchestratorCtx,
  agentId: string,
  taskId: string,
  resource: string,
  mode: LockMode,
): boolean {
  const lockId = `${resource}-lock`;
  let lock = ctx.state.locks[lockId];

  if (!lock) {
    lock = { id: lockId, resource, mode, queue: [] };
  }

  if (!lock.holderAgentId) {
    lock = { ...lock, holderAgentId: agentId, holderTaskId: taskId, mode };
    ctx.state.locks = { ...ctx.state.locks, [lockId]: lock };
    emitToCtx(ctx, {
      subsystem: "resource",
      type: "acquired",
      taskId,
      agentId,
      item: { kind: "lock", id: lockId, resource, mode },
      result: { kind: "lock", id: lockId, resource, granted: true },
    });
    return true;
  }

  if (lock.holderAgentId === agentId) {
    lock = { ...lock, mode, holderTaskId: taskId };
    ctx.state.locks = { ...ctx.state.locks, [lockId]: lock };
    return true;
  }

  if (mode === "read" && lock.mode === "read") {
    return true;
  }

  // Queue
  lock = { ...lock, queue: [...lock.queue, { agentId, taskId, mode }] };
  ctx.state.locks = { ...ctx.state.locks, [lockId]: lock };
  return false;
}

export function releaseLock(
  ctx: OrchestratorCtx,
  agentId: string,
  taskId: string,
  resource: string,
): void {
  const lockId = `${resource}-lock`;
  const lock = ctx.state.locks[lockId];
  if (!lock || lock.holderAgentId !== agentId) return;

  // Promote next waiter
  if (lock.queue.length > 0) {
    const next = lock.queue.shift()!;
    const newLock: LockState = {
      ...lock,
      holderAgentId: next.agentId,
      holderTaskId: next.taskId,
      mode: next.mode,
      queue: [...lock.queue],
    };
    ctx.state.locks = { ...ctx.state.locks, [lockId]: newLock };
    emitToCtx(ctx, {
      subsystem: "resource",
      type: "acquired",
      taskId: next.taskId,
      agentId: next.agentId,
      item: { kind: "lock", id: lockId, resource, mode: next.mode },
      result: { kind: "lock", id: lockId, resource, granted: true },
    });
  } else {
    ctx.state.locks = {
      ...ctx.state.locks,
      [lockId]: { ...lock, holderAgentId: undefined, holderTaskId: undefined, queue: [] },
    };
  }
}
