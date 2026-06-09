// ---- Scheduler: promote idle agents with inbox tasks to running ----

import type { OrchestratorCtx } from "./context.js";
import { emitToCtx } from "./context.js";

/**
 * Schedule one tick: scan all idle agents with inbox tasks,
 * start them (status → running) subject to lock constraints.
 */
export function schedule(ctx: OrchestratorCtx): void {
  if (ctx.state.status !== "running") return;

  const runnable = Object.values(ctx.state.agents).filter(
    (a) => a.inbox.length > 0 && a.status === "idle",
  );

  runnable.sort((_a, _b) => {
    const aTask = ctx.state.tasks[_a.inbox[0]];
    const bTask = ctx.state.tasks[_b.inbox[0]];
    return (bTask?.priority ?? 0) - (aTask?.priority ?? 0);
  });

  for (const agent of runnable) {
    const concurrency = agent.spec.concurrency;

    if (concurrency?.requiresWriteLock) {
      const writeLock = Object.values(ctx.state.locks).find(
        (l) => l.resource === "workspace" && l.mode === "write" && l.holderAgentId,
      );
      if (writeLock) {
        emitToCtx(ctx, {
          type: "scheduler_decision",
          decision: { kind: "deferred", agentId: agent.id, reason: "lock_unavailable" },
        });
        continue;
      }
    }

    if (concurrency?.maxConcurrentTasks !== undefined && concurrency.maxConcurrentTasks <= 0) {
      emitToCtx(ctx, {
        type: "scheduler_decision",
        decision: { kind: "deferred", agentId: agent.id, reason: "agent_busy" },
      });
      continue;
    }

    const taskId = agent.inbox[0];
    const task = ctx.state.tasks[taskId];
    if (!task) continue;

    ctx.state.tasks = { ...ctx.state.tasks, [taskId]: { ...task, status: "running" } };
    ctx.state.agents = {
      ...ctx.state.agents,
      [agent.id]: {
        ...agent,
        status: "running",
        activeTaskId: taskId,
        inbox: agent.inbox.slice(1),
      },
    };

    emitToCtx(ctx, {
      type: "agent_status_changed",
      agentId: agent.id,
      from: "idle",
      to: "running",
    });
    emitToCtx(ctx, { type: "task_started", taskId, agentId: agent.id });
    emitToCtx(ctx, {
      type: "scheduler_decision",
      decision: { kind: "started", agentId: agent.id, taskId },
    });
  }

  // Emit skipped for non-idle blocked agents
  for (const agent of Object.values(ctx.state.agents)) {
    if (agent.inbox.length > 0 && agent.status !== "idle") {
      emitToCtx(ctx, {
        type: "scheduler_decision",
        decision: { kind: "skipped", agentId: agent.id, reason: "agent_busy" },
      });
    }
  }

  if (runnable.length === 0) {
    const hasQueued = Object.values(ctx.state.agents).some((a) => a.inbox.length > 0);
    if (hasQueued) {
      emitToCtx(ctx, {
        type: "scheduler_decision",
        decision: { kind: "skipped", reason: "no_tasks" },
      });
    }
  }
}
