// ---- ToolRegistry tests — DI container + discovery ----

import { describe, expect, it } from "bun:test";
import type {
  ApprovalGateway,
  ToolApprovalDecision,
  ToolApprovalRequest,
  ToolDef,
  ToolSet,
} from "piko-orchestrator-protocol";
import type { OrchestratorEvent } from "../../src/actors/state/index.js";
import { ActorSystem } from "../../src/kernel/actor-system.js";
import { ToolRegistryImpl } from "../../src/tools/index.js";
import { createMockToolProvider } from "../helpers/index.js";

function makeToolDef(name: string): ToolDef {
  return {
    name,
    description: `Tool: ${name}`,
    inputSchema: { type: "object", properties: {} },
    executor: { kind: "native", target: name },
  };
}

function makeToolSet(id: string, tools: ToolSet["tools"] = []): ToolSet {
  return { id, name: `ToolSet ${id}`, tools };
}

describe("ToolRegistry", () => {
  function createRegistry() {
    const system = new ActorSystem();
    const emitted: OrchestratorEvent[] = [];
    const emit = async (e: OrchestratorEvent) => {
      emitted.push(e);
    };
    const registry = new ToolRegistryImpl(system, emit);
    return { system, registry, emitted };
  }

  // ---- Registration ----

  it("registerProvider adds to providers map", () => {
    const { registry } = createRegistry();
    const provider = createMockToolProvider({ id: "test-provider" });
    registry.registerProvider(provider);
    expect(registry.providers.get("test-provider")).toBe(provider);
  });

  it("registerToolSet adds to toolSets map", () => {
    const { registry } = createRegistry();
    const toolSet = makeToolSet("ts:test");
    registry.registerToolSet(toolSet);
    expect(registry.toolSets.get("ts:test")).toBe(toolSet);
  });

  it("unregisterToolSet removes from map", () => {
    const { registry } = createRegistry();
    const toolSet = makeToolSet("ts:test");
    registry.registerToolSet(toolSet);
    registry.unregisterToolSet("ts:test");
    expect(registry.toolSets.has("ts:test")).toBe(false);
  });

  // ---- Discovery (direct call, not actor) ----

  it("discoverTools returns tools from registered providers filtered by ToolSet", async () => {
    const { registry } = createRegistry();

    registry.registerProvider(
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("bash"), makeToolDef("read"), makeToolDef("grep")],
      }),
    );
    registry.registerToolSet(
      makeToolSet("ts:default", [
        { kind: "provider_tool", providerId: "engine", toolName: "bash" },
        { kind: "provider_tool", providerId: "engine", toolName: "read" },
      ]),
    );

    const { tools, routes } = await registry.discoverTools({
      agentId: "agent-1",
      toolSetIds: ["ts:default"],
    });

    expect(tools.length).toBe(2);
    expect(tools.map((t) => t.name).sort()).toEqual(["bash", "read"]);
    expect(routes.has("bash")).toBe(true);
    expect(routes.has("read")).toBe(true);
    expect(routes.get("bash")!.providerId).toBe("engine");
  });

  it("discoverTools applies activeToolNames filter", async () => {
    const { registry } = createRegistry();

    registry.registerProvider(
      createMockToolProvider({
        id: "engine",
        tools: [makeToolDef("bash"), makeToolDef("read"), makeToolDef("grep")],
      }),
    );
    registry.registerToolSet(
      makeToolSet("ts:default", [
        { kind: "provider_tool", providerId: "engine", toolName: "bash" },
        { kind: "provider_tool", providerId: "engine", toolName: "read" },
        { kind: "provider_tool", providerId: "engine", toolName: "grep" },
      ]),
    );

    const { tools } = await registry.discoverTools({
      agentId: "agent-1",
      toolSetIds: ["ts:default"],
      activeToolNames: ["read", "grep"],
    });

    expect(tools.map((t) => t.name).sort()).toEqual(["grep", "read"]);
  });

  // ---- Spawn / stop ----

  it("spawnToolActor creates an actor in the system", () => {
    const { system, registry } = createRegistry();
    const toolId = registry.spawnToolActor("tool:test:step_1");
    expect(toolId).toBe("tool:test:step_1");
    expect(system.hasActor("tool:test:step_1")).toBe(true);
  });

  it("stopToolActor removes the actor", async () => {
    const { system, registry } = createRegistry();
    registry.spawnToolActor("tool:test:step_1");
    await registry.stopToolActor("tool:test:step_1");
    expect(system.hasActor("tool:test:step_1")).toBe(false);
  });

  it("setApprovalGateway shares reference to new ToolActor instances", () => {
    const { registry } = createRegistry();
    const gateway: ApprovalGateway = {
      requestToolApproval: async (_req: ToolApprovalRequest): Promise<ToolApprovalDecision> =>
        "accept",
    };
    registry.setApprovalGateway(gateway);
    expect(registry.approvalGateway).toBe(gateway);
  });
});
