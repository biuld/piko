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
    turnIndex: 0,
    plan,
  });
}

export function subscribe(ctx: OrchestratorContext, listener: HostEventListener): () => void {
  return ctx.eventStore.subscribe(listener);
}

export function snapshot(ctx: OrchestratorContext): OrchState {
  return ctx.eventStore.snapshot();
}

export async function getGraph(ctx: OrchestratorContext): Promise<{
  nodes: Array<{ id: string; label: string; kind: string; status?: string }>;
  edges: Array<{ from: string; to: string; label?: string }>;
}> {
  return ctx.eventStore.graph();
}
