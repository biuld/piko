// ---- Registry: ToolSet & Agent management ----

import type { EngineToolSet } from "piko-engine-protocol";
import type { AgentRuntimeState, AgentSpec } from "piko-orchestrator-protocol";
import type { OrchestratorCtx } from "../context.js";

export function registerToolSet(ctx: OrchestratorCtx, toolSet: EngineToolSet): void {
  ctx.state.toolSets = { ...ctx.state.toolSets, [toolSet.id]: toolSet };
}

export function unregisterToolSet(ctx: OrchestratorCtx, toolSetId: string): void {
  delete (ctx.state as { toolSets: Record<string, EngineToolSet> }).toolSets[toolSetId];
  ctx.state = { ...ctx.state };
}

export function registerAgent(ctx: OrchestratorCtx, spec: AgentSpec): void {
  for (const tsId of spec.toolSetIds) {
    if (!ctx.state.toolSets[tsId]) {
      throw new Error(
        `Agent "${spec.id}" references unknown ToolSet "${tsId}". Register the ToolSet first.`,
      );
    }
  }

  const runtimeState: AgentRuntimeState = {
    id: spec.id,
    spec,
    status: "idle",
    inbox: [],
    transcript: [],
  };

  ctx.state.agents = { ...ctx.state.agents, [spec.id]: runtimeState };
}

export function unregisterAgent(ctx: OrchestratorCtx, agentId: string): void {
  delete (ctx.state as { agents: Record<string, AgentRuntimeState> }).agents[agentId];
  ctx.state = { ...ctx.state };
}
