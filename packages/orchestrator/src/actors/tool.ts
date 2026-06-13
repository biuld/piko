// ---- ToolActor — discovery, routing, execution coordination ----

import type {
  ApprovalGateway,
  ToolApprovalRequirement,
  ToolCall,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolPolicy,
  ToolProvider,
  ToolSet,
} from "piko-orchestrator-protocol";
import type { ActorHandler } from "../kernel/actor-system.js";
import type { OrchestratorEvent } from "./state.js";

// Re-export for convenience
export type { ToolCall, ToolDiscoveryContext, ToolExecResult, ToolExecutionContext, ToolProvider };

// ---- Messages ----

export type ToolMsg =
  | { type: "register_provider"; provider: ToolProvider }
  | { type: "unregister_provider"; providerId: string }
  | { type: "set_approval_gateway"; gateway?: ApprovalGateway }
  | {
      type: "register_tool_set";
      toolSet: ToolSet;
    }
  | { type: "unregister_tool_set"; toolSetId: string }
  | {
      type: "discover_tools";
      context: ToolDiscoveryContext;
    }
  | {
      type: "execute";
      call: ToolCall;
      context: ToolExecutionContext;
    }
  | { type: "cancel"; callId: string; reason?: string }
  | { type: "task_finished"; agentId: string; taskId: string };

// ---- ToolActor state ----

interface ToolActorState {
  providers: Map<string, ToolProvider>;
  /** All registered ToolSets. */
  toolSets: Map<string, ToolSet>;
  activeCalls: Map<string, { call: ToolCall; context: ToolExecutionContext }>;
  /** Runtime approval API owned by the host/orchestrator, not by tool providers. */
  approvalGateway?: ApprovalGateway;
}

interface CatalogEntry {
  publicName: string;
  providerId: string;
  providerToolName: string;
  toolDef: ToolDef;
}

// ---- ToolActor handler factory ----

export function toolActor(
  state: ToolActorState,
  deps: {
    emit: (event: OrchestratorEvent) => Promise<void>;
  },
): ActorHandler<ToolMsg> {
  return async (msg, ctx, meta) => {
    switch (msg.type) {
      case "register_provider": {
        state.providers.set(msg.provider.id, msg.provider);
        ctx.reply(meta, undefined);
        return;
      }

      case "unregister_provider": {
        state.providers.delete(msg.providerId);
        ctx.reply(meta, undefined);
        return;
      }

      case "set_approval_gateway": {
        state.approvalGateway = msg.gateway;
        ctx.reply(meta, undefined);
        return;
      }

      case "register_tool_set": {
        state.toolSets.set(msg.toolSet.id, msg.toolSet);
        ctx.reply(meta, undefined);
        return;
      }

      case "unregister_tool_set": {
        state.toolSets.delete(msg.toolSetId);
        ctx.reply(meta, undefined);
        return;
      }

      case "discover_tools": {
        const catalog = await buildCatalog(state, msg.context);

        // Apply active tool restrictions
        let result = catalog.map((entry) => entry.toolDef);
        if (msg.context.activeToolNames?.length) {
          result = result.filter((tool) => msg.context.activeToolNames!.includes(tool.name));
        }

        ctx.reply(meta, result);
        return;
      }

      case "execute": {
        const { call, context } = msg;
        state.activeCalls.set(call.id, { call, context });

        await deps.emit({
          type: "tool_started",
          agentId: context.agentId,
          taskId: context.taskId,
          callId: call.id,
          name: call.name,
          args: call.arguments,
        });

        const catalog = await buildCatalog(state, {
          agentId: context.agentId,
          taskId: context.taskId,
          toolSetIds: context.toolSetIds,
        });
        const route = catalog.find((entry) => entry.publicName === call.name);
        const provider = route ? state.providers.get(route.providerId) : undefined;

        if (!route || !provider) {
          const result: ToolExecResult = {
            ok: false,
            error: {
              code: "not_found",
              message: `No provider for tool "${call.name}"`,
            },
          };
          await deps.emit({
            type: "tool_finished",
            agentId: context.agentId,
            taskId: context.taskId,
            callId: call.id,
            result,
          });
          state.activeCalls.delete(call.id);
          ctx.reply(meta, result);
          return;
        }

        const providerCall: ToolCall =
          route.providerToolName !== call.name ? { ...call, name: route.providerToolName } : call;

        // ---- Approval check ----
        const effectiveApproval = route.toolDef.approval ?? "never";
        if (effectiveApproval !== "never" && state.approvalGateway) {
          const needsApproval =
            effectiveApproval === "always" || effectiveApproval === "on_request";

          if (needsApproval) {
            const decision = await state.approvalGateway.requestToolApproval({
              callId: call.id,
              agentId: context.agentId,
              taskId: context.taskId,
              toolName: route.publicName,
              toolArgs: providerCall.arguments,
            });

            if (decision === "decline") {
              const result: ToolExecResult = {
                ok: false,
                error: { code: "declined", message: "User declined approval" },
              };
              await deps.emit({
                type: "approval_resolved",
                approvalId: call.id,
                decision: "decline",
              });
              await deps.emit({
                type: "tool_finished",
                agentId: context.agentId,
                taskId: context.taskId,
                callId: call.id,
                result,
              });
              state.activeCalls.delete(call.id);
              ctx.reply(meta, result);
              return;
            }

            await deps.emit({
              type: "approval_resolved",
              approvalId: call.id,
              decision: "accept",
            });
          }
        }

        // Execute
        try {
          const result = await provider.execute(providerCall, context);

          await deps.emit({
            type: "tool_finished",
            agentId: context.agentId,
            taskId: context.taskId,
            callId: call.id,
            result,
          });

          state.activeCalls.delete(call.id);
          ctx.reply(meta, result);
        } catch (err) {
          const errorMsg = err instanceof Error ? err.message : String(err);
          const errorResult: ToolExecResult = {
            ok: false,
            error: {
              code: "execution_error",
              message: errorMsg,
            },
          };

          await deps.emit({
            type: "tool_finished",
            agentId: context.agentId,
            taskId: context.taskId,
            callId: call.id,
            result: errorResult,
          });

          state.activeCalls.delete(call.id);
          ctx.reply(meta, errorResult);
        }
        return;
      }

      case "cancel": {
        state.activeCalls.delete(msg.callId);
        ctx.reply(meta, undefined);
        return;
      }

      case "task_finished": {
        for (const [callId, active] of state.activeCalls) {
          if (active.context.taskId === msg.taskId) {
            state.activeCalls.delete(callId);
          }
        }
        ctx.reply(meta, undefined);
        return;
      }
    }
  };
}

// ---- Helpers ----

async function buildCatalog(
  state: ToolActorState,
  context: ToolDiscoveryContext,
): Promise<CatalogEntry[]> {
  const entries: CatalogEntry[] = [];
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  const providerTools = new Map<string, ToolDef[]>();

  const discoverProvider = async (providerId: string): Promise<ToolDef[]> => {
    const cached = providerTools.get(providerId);
    if (cached) return cached;

    const provider = state.providers.get(providerId);
    if (!provider) return [];

    const tools = await provider.discover({
      agentId: context.agentId,
      taskId: context.taskId,
      toolSetIds: [],
    });
    providerTools.set(providerId, tools);
    return tools;
  };

  const addEntry = (
    publicName: string,
    providerId: string,
    providerToolName: string,
    toolDef: ToolDef,
    policy?: Partial<ToolPolicy>,
  ): void => {
    if (seen.has(publicName)) {
      duplicates.add(publicName);
    }
    seen.add(publicName);
    entries.push({
      publicName,
      providerId,
      providerToolName,
      toolDef: projectToolDef(toolDef, publicName, policy),
    });
  };

  for (const toolSetId of context.toolSetIds) {
    const toolSet = state.toolSets.get(toolSetId);
    if (!toolSet) continue;

    for (const ref of toolSet.tools) {
      const policy = { ...toolSet.policy?.defaults, ...ref.policy };

      if (ref.kind === "provider_tool") {
        const tools = await discoverProvider(ref.providerId);
        const toolDef = tools.find((tool) => tool.name === ref.toolName);
        if (toolDef) {
          addEntry(ref.alias ?? ref.toolName, ref.providerId, ref.toolName, toolDef, policy);
        }
        continue;
      }

      if (ref.kind === "orchestrator_control") {
        const tools = await discoverProvider("orch");
        const toolDef = tools.find((tool) => tool.name === ref.action);
        if (toolDef) {
          addEntry(ref.alias ?? ref.action, "orch", ref.action, toolDef, policy);
        }
        continue;
      }

      const tools = await discoverProvider(ref.providerId);
      for (const toolDef of tools) {
        if (toolDef.name.startsWith(ref.namespace)) {
          addEntry(toolDef.name, ref.providerId, toolDef.name, toolDef, policy);
        }
      }
    }
  }

  if (duplicates.size > 0) {
    throw new Error(`Duplicate tool names in catalog: ${[...duplicates].sort().join(", ")}`);
  }

  return entries;
}

function projectToolDef(
  toolDef: ToolDef,
  publicName: string,
  policy?: Partial<ToolPolicy>,
): ToolDef {
  const projected: ToolDef = { ...toolDef, name: publicName };
  if (!policy) return projected;

  if (policy.approval) {
    projected.approval =
      policy.approval === "on_sensitive"
        ? "on_request"
        : (policy.approval as ToolApprovalRequirement);
  } else if (policy.sensitivity === "dangerous") {
    projected.approval = "always";
  } else if (policy.sensitivity === "sensitive" && !projected.approval) {
    projected.approval = "on_request";
  } else if (policy.sensitivity === "safe" && !projected.approval) {
    projected.approval = "never";
  }

  if (policy.executionMode) {
    projected.executionMode = policy.executionMode;
  }

  return projected;
}

// ---- Factory ----

export function createToolActor(deps: { emit: (event: OrchestratorEvent) => Promise<void> }) {
  const state: ToolActorState = {
    providers: new Map(),
    toolSets: new Map(),
    activeCalls: new Map(),
  };

  return {
    handler: toolActor(state, deps),
    state,
  };
}
