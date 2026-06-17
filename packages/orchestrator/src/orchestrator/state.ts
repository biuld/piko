import type { HostEventListener, OrchState } from "piko-orchestrator-protocol";
import type { OrchestratorContext } from "./context.js";

export function updatePlan(
  ctx: OrchestratorContext,
  agentId: string,
  taskId: string,
  plan: unknown[],
): void {
  void ctx.emit({
    type: "plan_updated",
    agentId,
    taskId,
    plan,
  });
}

export function subscribe(ctx: OrchestratorContext, listener: HostEventListener): () => void {
  const subPromise = ctx.system.ask<{ id: string; unsubscribe: () => void }>(ctx.stateRef, {
    type: "subscribe",
    listener,
  });
  let unsubscribed = false;
  let subId: string | undefined;

  subPromise
    .then((sub) => {
      subId = sub.id;
      if (unsubscribed) {
        ctx.system.send(ctx.stateRef, { type: "unsubscribe", subscriptionId: sub.id });
      }
    })
    .catch(() => {});

  return () => {
    unsubscribed = true;
    if (subId) {
      ctx.system.send(ctx.stateRef, { type: "unsubscribe", subscriptionId: subId });
    }
  };
}

export function snapshot(ctx: OrchestratorContext): OrchState {
  return structuredClone({
    runId: ctx.runId,
    status: ctx.stateCache.status,
    toolSets: ctx.stateCache.toolSets,
    agents: ctx.stateCache.agents,
    tasks: ctx.stateCache.tasks,
  });
}

export async function getGraph(ctx: OrchestratorContext): Promise<{
  nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
  edges: Array<{ from: string; to: string; label?: string }>;
}> {
  return ctx.system.ask<{
    nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
    edges: Array<{ from: string; to: string; label?: string }>;
  }>(ctx.stateRef, { type: "render_graph" });
}
