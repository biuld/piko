// ---- ToolRegistry — DI container for tool lifecycle ----
//
// Responsibilities:
//  - Hold singleton references to all registered providers, toolSets, and approval gateway
//  - discoverTools(): pure computation over shared state (no actor messages)
//  - Spawn prototype ToolActors on demand, injecting singleton dependencies at construction time
//
// This is NOT an actor — no mailbox, no serialization, no messages.
// Writes (registerProvider etc.) are synchronous mutations on shared Maps.

import type {
  ApprovalGateway,
  ToolApprovalRequirement,
  ToolDef,
  ToolDiscoveryContext,
  ToolPolicy,
  ToolProvider,
  ToolSet,
} from "piko-orchestrator-protocol";
import type { OrchestratorEvent } from "../actors/state/index.js";
import type { CatalogRoute } from "../actors/tool.js";
import { createToolActor } from "../actors/tool.js";
import type { ActorHandler, ActorSystem } from "../kernel/actor-system.js";

// ---- Public interface (used by AgentActorDeps) ----

export interface ToolRegistry {
  /** Discover tools for the given context. Pure async function, not an actor message. */
  discoverTools(
    context: ToolDiscoveryContext,
  ): Promise<{ tools: ToolDef[]; routes: Map<string, CatalogRoute> }>;

  /** Spawn a fresh ToolActor with all current singleton deps injected at construction. */
  spawnToolActor(id: string): string;

  /** Stop a previously spawned ToolActor. */
  stopToolActor(id: string): Promise<void>;
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

  // ---- Actor system access (for spawn/stop) ----
  private system: ActorSystem;
  private emit: (event: OrchestratorEvent) => Promise<void>;

  constructor(system: ActorSystem, emit: (event: OrchestratorEvent) => Promise<void>) {
    this.system = system;
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

  // ---- Prototype bean factory ----

  spawnToolActor(id: string): string {
    const actor = createToolActor({
      emit: this.emit,
      providers: this.providers, // shared reference
      approvalGateway: this.approvalGateway,
    });
    this.system.spawn({
      id,
      kind: "tool",
      handler: actor.handler as ActorHandler,
    });
    return id;
  }

  stopToolActor(id: string): Promise<void> {
    return this.system.stop(id);
  }
}

// ---- Catalog builder (moved from ToolActor) ----

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
