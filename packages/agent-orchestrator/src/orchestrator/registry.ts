// ---- Registry: ToolSet & Agent management ----

import type { AgentRuntimeState, AgentSpec, EngineToolSet } from "piko-engine-protocol";
import type { OrchestratorCtx } from "./context.js";
import { emitToCtx } from "./context.js";

export function registerToolSet(ctx: OrchestratorCtx, toolSet: EngineToolSet): void {
  ctx.state.toolSets = { ...ctx.state.toolSets, [toolSet.id]: toolSet };
  emitToCtx(ctx, { type: "toolset_registered", toolSetId: toolSet.id, name: toolSet.name });
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
  emitToCtx(ctx, {
    type: "agent_registered",
    agentId: spec.id,
    name: spec.name,
    role: spec.role,
    toolSetIds: spec.toolSetIds,
  });
}

export function unregisterAgent(ctx: OrchestratorCtx, agentId: string): void {
  delete (ctx.state as { agents: Record<string, AgentRuntimeState> }).agents[agentId];
  ctx.state = { ...ctx.state };
  emitToCtx(ctx, { type: "agent_unregistered", agentId });
}
