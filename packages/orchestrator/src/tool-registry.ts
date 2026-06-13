// ---- ToolRegistry — DI container for tool lifecycle ----
//
// Responsibilities:
//  - Hold singleton references to all registered providers, toolSets, and approval gateway
//  - Spawn prototype ToolActors on demand, injecting singleton dependencies at construction time
//
// This is NOT an actor — no mailbox, no serialization, no messages.
// Writes (registerProvider etc.) are synchronous mutations on shared Maps.
// Reads (spawnToolActor) read current state and spawn via ActorSystem.

import type { ApprovalGateway, ToolProvider, ToolSet } from "piko-orchestrator-protocol";
import type { OrchestratorEvent } from "./actors/state.js";
import { createToolActor } from "./actors/tool.js";
import type { ActorHandler, ActorSystem } from "./kernel/actor-system.js";

// ---- Public interface (used by AgentActorDeps) ----

export interface ToolRegistry {
  /** Spawn a fresh ToolActor with all current singleton deps injected at construction. */
  spawnToolActor(id: string): string;

  /** Stop a previously spawned ToolActor. */
  stopToolActor(id: string): Promise<void>;
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

  // ---- Prototype bean factory ----

  spawnToolActor(id: string): string {
    const actor = createToolActor({
      emit: this.emit,
      providers: this.providers, // shared reference
      toolSets: this.toolSets, // shared reference
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
