import type { OrchestratorCtx } from "../context.js";
import { emitToCtx } from "../context.js";

export function start(ctx: OrchestratorCtx): void {
  if (ctx.state.status === "running") return;
  emitToCtx(ctx, {
    subsystem: "lifecycle",
    type: "orchestrator_started",
    runId: ctx.state.runId,
  });
}

export function stop(ctx: OrchestratorCtx): void {
  if (ctx.state.status !== "running") return;
  emitToCtx(ctx, {
    subsystem: "lifecycle",
    type: "orchestrator_stopped",
    runId: ctx.state.runId,
    reason: "manual",
  });
}
