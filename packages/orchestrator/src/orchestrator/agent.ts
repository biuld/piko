import type { AgentSpec } from "piko-orchestrator-protocol";
import type { OrchestratorContext } from "./context.js";

export function registerAgent(ctx: OrchestratorContext, spec: AgentSpec): void {
  ctx.agentSpecs.set(spec.id, spec);
  void ctx.emit({ type: "agent_registered", agent: spec });
}

export function unregisterAgent(ctx: OrchestratorContext, agentId: string): void {
  ctx.agentSpecs.delete(agentId);
  void ctx.emit({
    type: "agent_unregistered",
    agentId,
  });
}
