import type { ApprovalGateway, ToolProvider, ToolSet } from "piko-orchestrator-protocol";
import type { OrchestratorContext } from "./context.js";

export function registerToolSet(ctx: OrchestratorContext, toolSet: ToolSet): void {
  ctx.toolRegistry.registerToolSet(toolSet);
  void ctx.emit({ type: "tool_set_registered", toolSet });
}

export function unregisterToolSet(ctx: OrchestratorContext, toolSetId: string): void {
  ctx.toolRegistry.unregisterToolSet(toolSetId);
  void ctx.emit({ type: "tool_set_unregistered", toolSetId });
}

export function setModelConfig(ctx: OrchestratorContext, config: any): void {
  for (const id of ctx.system.getActorIds()) {
    if (id.startsWith("agent:")) {
      try {
        ctx.system.send(id, {
          type: "set_model_config",
          config,
        });
      } catch {
        // Agent may have stopped
      }
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
