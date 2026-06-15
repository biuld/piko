import type { ApprovalGateway, ToolProvider, ToolSet } from "piko-orchestrator-protocol";
import type { OrchestratorContext } from "./context.js";

export function registerToolSet(ctx: OrchestratorContext, toolSet: ToolSet): void {
  ctx.toolRegistry.registerToolSet(toolSet);
  ctx.stateCache.toolSets[toolSet.id] = toolSet;
}

export function unregisterToolSet(ctx: OrchestratorContext, toolSetId: string): void {
  ctx.toolRegistry.unregisterToolSet(toolSetId);
  delete ctx.stateCache.toolSets[toolSetId];
}

export function setModelConfig(ctx: OrchestratorContext, config: any): void {
  for (const agentId of Object.keys(ctx.stateCache.agents)) {
    try {
      ctx.system.send(`agent:${agentId}`, {
        type: "set_model_config",
        config,
      });
    } catch {
      // Agent may not be spawned yet
    }
  }
}

export function setApprovalGateway(
  ctx: OrchestratorContext,
  gateway: ApprovalGateway | undefined,
): void {
  ctx.toolRegistry.setApprovalGateway(gateway);
}

export function registerProvider(ctx: OrchestratorContext, provider: ToolProvider): void {
  ctx.toolRegistry.registerProvider(provider);
}
