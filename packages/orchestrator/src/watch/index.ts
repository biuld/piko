// ---- Watch management ----

import type { AgentWatch, AgentWatchId, AgentWatchState } from "piko-orchestrator-protocol";
import type { OrchestratorCtx } from "../context.js";
import { v4Id } from "../id.js";
import { dispatch } from "../task/index.js";

export function registerWatch(ctx: OrchestratorCtx, watch: AgentWatch): AgentWatchId {
  const id = watch.id ?? v4Id("watch");
  const ws: AgentWatchState = { id, agentId: watch.agentId, kind: watch.kind, active: true };
  ctx.state.watches = { ...ctx.state.watches, [id]: ws };

  if (watch.kind === "interval") {
    startIntervalWatch(ctx, id, watch);
  }

  return id;
}

export function unregisterWatch(ctx: OrchestratorCtx, watchId: AgentWatchId): void {
  const ws = ctx.state.watches[watchId];
  if (!ws) return;
  ctx.state.watches = { ...ctx.state.watches, [watchId]: { ...ws, active: false } };
}

function startIntervalWatch(
  ctx: OrchestratorCtx,
  watchId: string,
  watch: Extract<AgentWatch, { kind: "interval" }>,
): void {
  const interval = setInterval(() => {
    const ws = ctx.state.watches[watchId];
    if (!ws?.active) {
      clearInterval(interval);
      return;
    }
    dispatch(ctx, {
      targetAgentId: watch.agentId,
      prompt: watch.prompt,
      source: { kind: "timer", watchId },
    }).catch(() => {});
  }, watch.everyMs);
}
