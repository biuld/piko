import type { AgentSpec } from "piko-orchestrator-protocol";
import { agentActor } from "../actors/agent/index.js";
import type { ActorHandler } from "../kernel/actor-system.js";
import type { OrchestratorContext } from "./context.js";

export function registerAgent(ctx: OrchestratorContext, spec: AgentSpec): void {
  const handler = agentActor(spec, ctx.createAgentDeps());
  ctx.system.spawn({
    id: `agent:${spec.id}`,
    kind: "agent",
    handler: handler as ActorHandler,
  });

  if (ctx.latestModelConfig) {
    ctx.system.send(`agent:${spec.id}`, {
      type: "set_model_config",
      config: ctx.latestModelConfig,
    });
  }

  // Update state cache synchronously to prevent event lag in snapshot checks
  const existing = ctx.stateCache.agents[spec.id];
  ctx.stateCache.agents[spec.id] = {
    id: spec.id,
    spec,
    status: "idle",
    transcript: existing?.transcript ?? [],
  };

  void ctx.emit({ type: "agent_registered", agent: spec });
}

export function unregisterAgent(ctx: OrchestratorContext, agentId: string): void {
  delete ctx.stateCache.agents[agentId];

  void ctx.system.stop(`agent:${agentId}`);
  void ctx.emit({
    type: "agent_unregistered",
    agentId,
  });
}
