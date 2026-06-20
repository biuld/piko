// ---- ToolRegistry — DI container and execution manager for tools ----
//
// Responsibilities:
//  - Hold singleton references to all registered providers, toolSets, and approval gateway
//  - discoverTools(): pure computation over shared state (no actor messages)
//  - executeTool(): execute a tool on a provider, applying policy, approvals, and mapping
//
// This is NOT an actor — no mailbox, no serialization, no messages.
// Writes (registerProvider etc.) are synchronous mutations on shared Maps.

import type {
  ApprovalGateway,
  ToolApprovalDecision,
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
import { startDebugSpan } from "piko-orchestrator-protocol";
import type { OrchestratorEvent } from "../actors/state/index.js";

export interface CatalogRoute {
  providerId: string;
  providerToolName: string;
  toolDef: ToolDef;
}

// ---- Public interface (used by AgentActorDeps) ----

export interface ToolRegistry {
  /** Discover tools for the given context. Pure async function, not an actor message. */
  discoverTools(
    context: ToolDiscoveryContext,
  ): Promise<{ tools: ToolDef[]; routes: Map<string, CatalogRoute> }>;

  executeTool(
    call: ToolCall,
    context: ToolExecutionContext,
    route: CatalogRoute,
    signal?: AbortSignal,
  ): Promise<ToolExecResult>;
}

// ---- Internal catalog types ----

interface CatalogEntry {
  publicName: string;
  providerId: string;
  providerToolName: string;
  toolDef: ToolDef;
}

// ---- Implementation (owned by Orchestrator facade) ----

export class ToolRegistryImpl implements ToolRegistry {
  // ---- Singleton beans ----
  readonly providers = new Map<string, ToolProvider>();
  readonly toolSets = new Map<string, ToolSet>();
  approvalGateway?: ApprovalGateway;
  private emit: (event: OrchestratorEvent) => Promise<void>;

  constructor(emit: (event: OrchestratorEvent) => Promise<void>) {
    this.emit = emit;
  }

  // ---- Singleton registration (synchronous, no actor messages) ----

  registerProvider(provider: ToolProvider): void {
    this.providers.set(provider.id, provider);
  }

  registerToolSet(toolSet: ToolSet): void {
    this.toolSets.set(toolSet.id, toolSet);
  }

  unregisterToolSet(toolSetId: string): void {
    this.toolSets.delete(toolSetId);
  }

  setApprovalGateway(gateway: ApprovalGateway | undefined): void {
    this.approvalGateway = gateway;
  }

  // ---- Discovery (direct call, not an actor message) ----

  async discoverTools(context: ToolDiscoveryContext): Promise<{
    tools: ToolDef[];
    routes: Map<string, CatalogRoute>;
  }> {
    const catalog = await buildCatalog(this.providers, this.toolSets, context);

    // Apply active tool restrictions
    let tools = catalog.map((entry) => entry.toolDef);
    if (context.activeToolNames?.length) {
      tools = tools.filter((tool) => context.activeToolNames!.includes(tool.name));
    }

    // Build route map for fast lookup during execution
    const routes = new Map<string, CatalogRoute>();
    for (const entry of catalog) {
      routes.set(entry.publicName, {
        providerId: entry.providerId,
        providerToolName: entry.providerToolName,
        toolDef: entry.toolDef,
      });
    }

    return { tools, routes };
  }

  async executeTool(
    call: ToolCall,
    context: ToolExecutionContext,
    route: CatalogRoute,
    signal?: AbortSignal,
  ): Promise<ToolExecResult> {
    const startEventSeq = context.nextEventSeq?.() ?? context.eventSeq ?? 0;
    await this.emit({
      type: "tool_started",
      agentId: context.agentId,
      taskId: context.taskId,
      eventSeq: startEventSeq,
      turnIndex: context.turnIndex ?? 0,
      parentMessageId: context.parentMessageId ?? "",
      contentIndex: context.contentIndex ?? 0,
      toolCallIndex: context.toolCallIndex ?? 0,
      callId: call.id,
      name: call.name,
      args: call.arguments,
    });

    const provider = this.providers.get(route.providerId);
    if (!provider) {
      const result: ToolExecResult = {
        ok: false,
        error: {
          code: "not_found",
          message: `No provider "${route.providerId}" for tool "${call.name}"`,
        },
      };
      await this.emitToolFinished(context, call.id, result);
      return result;
    }

    const providerCall: ToolCall =
      route.providerToolName !== call.name ? { ...call, name: route.providerToolName } : call;

    const effectiveApproval = route.toolDef.approval ?? "never";
    if (effectiveApproval !== "never" && this.approvalGateway) {
      const needsApproval = effectiveApproval === "always" || effectiveApproval === "on_request";

      if (needsApproval) {
        if (signal?.aborted) {
          const result: ToolExecResult = {
            ok: false,
            error: { code: "aborted", message: "Task cancelled" },
          };
          await this.emitToolFinished(context, call.id, result);
          return result;
        }

        const approvalSpan = startDebugSpan("tool.approval", {
          taskId: context.taskId,
          agentId: context.agentId,
          toolCallId: call.id,
          toolName: call.name,
          signalAborted: signal?.aborted ?? false,
        });
        await this.emit({
          type: "approval_requested",
          approvalId: call.id,
          agentId: context.agentId,
          taskId: context.taskId,
          toolName: call.name,
          toolArgs: providerCall.arguments,
          eventSeq: context.nextEventSeq?.() ?? context.eventSeq ?? 0,
          turnIndex: context.turnIndex ?? 0,
        });
        const decisionPromise = this.approvalGateway.requestToolApproval(
          {
            callId: call.id,
            agentId: context.agentId,
            taskId: context.taskId,
            toolName: call.name,
            toolArgs: providerCall.arguments,
          },
          signal,
        );

        let decision: ToolApprovalDecision;

        if (signal) {
          let onAbort: () => void;
          const abortPromise = new Promise<never>((_, reject) => {
            onAbort = () => reject(new Error("aborted"));
            signal.addEventListener("abort", onAbort, { once: true });
          });

          try {
            decision = await Promise.race([decisionPromise, abortPromise]);
            signal.removeEventListener("abort", onAbort!);
            approvalSpan.end({ outcome: "completed", status: decision });
          } catch (_err) {
            signal.removeEventListener("abort", onAbort!);
            approvalSpan.end({
              outcome: signal.aborted ? "aborted" : "error",
              signalAborted: signal.aborted,
            });
            if (signal.aborted) {
              const result: ToolExecResult = {
                ok: false,
                error: { code: "aborted", message: "Task cancelled" },
              };
              await this.emit({
                type: "approval_resolved",
                approvalId: call.id,
                agentId: context.agentId,
                taskId: context.taskId,
                eventSeq: context.nextEventSeq?.() ?? context.eventSeq ?? 0,
                turnIndex: context.turnIndex ?? 0,
                decision: "decline",
              });
              await this.emitToolFinished(context, call.id, result);
              return result;
            }
            const errorMsg = _err instanceof Error ? _err.message : String(_err);
            const result: ToolExecResult = {
              ok: false,
              error: { code: "execution_error", message: `Approval gateway error: ${errorMsg}` },
            };
            await this.emitToolFinished(context, call.id, result);
            return result;
          }
        } else {
          try {
            decision = await decisionPromise;
            approvalSpan.end({ outcome: "completed", status: decision });
          } catch (_err) {
            approvalSpan.end({ outcome: "error" });
            const errorMsg = _err instanceof Error ? _err.message : String(_err);
            const result: ToolExecResult = {
              ok: false,
              error: { code: "execution_error", message: `Approval gateway error: ${errorMsg}` },
            };
            await this.emitToolFinished(context, call.id, result);
            return result;
          }
        }

        if (decision === "decline") {
          const result: ToolExecResult = {
            ok: false,
            error: { code: "declined", message: "User declined approval" },
          };
          await this.emit({
            type: "approval_resolved",
            approvalId: call.id,
            agentId: context.agentId,
            taskId: context.taskId,
            eventSeq: context.nextEventSeq?.() ?? context.eventSeq ?? 0,
            turnIndex: context.turnIndex ?? 0,
            decision: "decline",
          });
          await this.emitToolFinished(context, call.id, result);
          return result;
        }

        await this.emit({
          type: "approval_resolved",
          approvalId: call.id,
          agentId: context.agentId,
          taskId: context.taskId,
          eventSeq: context.nextEventSeq?.() ?? context.eventSeq ?? 0,
          turnIndex: context.turnIndex ?? 0,
          decision: "accept",
        });
      }
    }

    const providerSpan = startDebugSpan("tool.provider.execute", {
      taskId: context.taskId,
      agentId: context.agentId,
      toolCallId: call.id,
      toolName: call.name,
      signalAborted: signal?.aborted ?? false,
    });
    try {
      const result = await provider.execute(providerCall, context, signal);
      providerSpan.end({ outcome: result.error ? "error" : "completed" });
      await this.emitToolFinished(context, call.id, result);
      return result;
    } catch (err) {
      providerSpan.end({
        outcome: signal?.aborted ? "aborted" : "error",
        signalAborted: signal?.aborted ?? false,
      });
      const errorMsg = err instanceof Error ? err.message : String(err);
      const errorResult: ToolExecResult = {
        ok: false,
        error: {
          code: "execution_error",
          message: errorMsg,
        },
      };
      await this.emitToolFinished(context, call.id, errorResult);
      return errorResult;
    }
  }

  private async emitToolFinished(
    context: ToolExecutionContext,
    callId: string,
    result: ToolExecResult,
  ): Promise<void> {
    const endEventSeq = context.nextEventSeq?.() ?? context.eventSeq ?? 0;
    await this.emit({
      type: "tool_finished",
      agentId: context.agentId,
      taskId: context.taskId,
      eventSeq: endEventSeq,
      turnIndex: context.turnIndex ?? 0,
      parentMessageId: context.parentMessageId ?? "",
      contentIndex: context.contentIndex ?? 0,
      toolCallIndex: context.toolCallIndex ?? 0,
      callId,
      result,
    });
  }
}

// ---- Catalog builder ----

async function buildCatalog(
  providers: Map<string, ToolProvider>,
  toolSets: Map<string, ToolSet>,
  context: ToolDiscoveryContext,
): Promise<CatalogEntry[]> {
  const entries: CatalogEntry[] = [];
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  const providerTools = new Map<string, ToolDef[]>();

  const discoverProvider = async (providerId: string): Promise<ToolDef[]> => {
    const cached = providerTools.get(providerId);
    if (cached) return cached;

    const provider = providers.get(providerId);
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
    const toolSet = toolSets.get(toolSetId);
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
