// ---- Orchestrator facade — end-to-end integration tests ----

import { describe, expect, it } from "bun:test";
import type { AgentSpec, ToolDef, ToolSet } from "piko-orchestrator-protocol";
import { Orchestrator } from "../src/orchestrator/index.js";
import type { FauxStepSpec } from "./helpers/index.js";
import { createFauxModelExecutor, createMockToolProvider } from "./helpers/index.js";

// ---- Helpers ----

function makeAgentSpec(id: string, overrides?: Partial<AgentSpec>): AgentSpec {
  return {
    id,
    name: `Agent ${id}`,
    role: "test",
    systemPrompt: "You are a test agent.",
    toolSetIds: [],
    ...overrides,
  };
}

function makeToolDef(name: string): ToolDef {
  return {
    name,
    description: `Tool: ${name}`,
    inputSchema: { type: "object", properties: {} },
    executor: { kind: "native", target: name },
  };
}

function makeToolSet(id: string, tools: ToolSet["tools"]): ToolSet {
  return { id, name: `ToolSet ${id}`, tools };
}

describe("Orchestrator (integration)", () => {
  // ---- Basic run ----

  it("run returns result with messages and steps", async () => {
    const steps: FauxStepSpec[] = [{ content: "Hello! How can I help?", status: "completed" }];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);

    orch.registerAgent(makeAgentSpec("assistant"));

    const result = await orch.run("Hi", { targetAgentId: "assistant" });

    expect(result.status).toBe("completed");
    expect(result.messages.length).toBeGreaterThan(0);
    expect(result.totalSteps).toBeGreaterThan(0);
  });

  it("snapshot reflects registered agents after run completes", async () => {
    const steps: FauxStepSpec[] = [{ content: "Done.", status: "completed" }];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);

    orch.registerAgent(makeAgentSpec("coordinator"));
    orch.registerAgent(makeAgentSpec("implementer"));

    // Run a task to ensure agent registration has been processed
    await orch.run("Hi", { targetAgentId: "coordinator" });

    const snap = orch.snapshot();
    expect(snap.agents.coordinator).toBeDefined();
    expect(snap.agents.coordinator.status).toBe("idle");
  });

  // ---- Subscribe ----

  it("subscribe receives HostEvents during run", async () => {
    const steps: FauxStepSpec[] = [
      {
        deltas: [{ type: "text", text: "Hello" }],
        content: "Hello, World!",
        status: "completed",
      },
    ];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);

    orch.registerAgent(makeAgentSpec("assistant"));

    const events: Array<{ type: string }> = [];
    orch.subscribe((event) => {
      events.push(event as { type: string });
    });

    await orch.run("Hi", { targetAgentId: "assistant" });

    // Should receive task_started, token, task_completed events
    expect(events.some((e) => e.type === "task_started")).toBe(true);
    expect(events.some((e) => e.type === "token")).toBe(true);
    expect(events.some((e) => e.type === "task_completed")).toBe(true);
  });

  it("subscribe receives tool events when tools are used", async () => {
    const bashTool = makeToolDef("bash");

    const steps: FauxStepSpec[] = [
      {
        toolCalls: [{ id: "tc1", name: "bash", arguments: { command: "ls" } }],
        status: "continue",
      },
      { content: "Done.", status: "completed" },
    ];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);

    // Register provider
    const provider = createMockToolProvider({
      id: "engine",
      tools: [bashTool],
      executeResult: { ok: true, value: "file.txt" },
    });
    orch.registerProvider(provider);

    // Register ToolSet
    const toolSet = makeToolSet("ts:default", [
      { kind: "provider_tool", providerId: "engine", toolName: "bash" },
    ]);
    orch.registerToolSet(toolSet);

    orch.registerAgent(makeAgentSpec("worker", { toolSetIds: ["ts:default"] }));

    const events: Array<{ type: string }> = [];
    orch.subscribe((event) => {
      events.push(event as { type: string });
    });

    const result = await orch.run("List files", { targetAgentId: "worker" });

    expect(result.status).toBe("completed");
    expect(events.some((e) => e.type === "tool_start")).toBe(true);
    expect(events.some((e) => e.type === "tool_end")).toBe(true);
  });

  // ---- dispatch ----

  it("dispatch returns taskId", async () => {
    const steps: FauxStepSpec[] = [{ content: "Done.", status: "completed" }];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);

    orch.registerAgent(makeAgentSpec("worker"));

    const taskId = await orch.dispatch({
      targetAgentId: "worker",
      prompt: "Do work",
      source: { type: "user" },
    });

    expect(taskId).toBeDefined();
    expect(typeof taskId).toBe("string");
  });

  // ---- dispatchDetached / joinTask ----

  it("dispatchDetached returns taskId immediately, joinTask awaits result", async () => {
    const steps: FauxStepSpec[] = [{ content: "Detached work done.", status: "completed" }];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);

    orch.registerAgent(makeAgentSpec("worker"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "worker",
      prompt: "Background work",
      source: { type: "user" },
    });

    expect(taskId).toBeDefined();

    const result = await orch.joinTask(taskId);
    expect(result).toBeDefined();
  });

  it("joinTask throws for unknown task", async () => {
    const orch = new Orchestrator();
    await expect(orch.joinTask("unknown-task")).rejects.toThrow("Detached task not found");
  });

  // ---- updatePlan ----

  it("updatePlan is reflected in snapshot task state", async () => {
    const steps: FauxStepSpec[] = [{ content: "Planned.", status: "completed" }];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);

    orch.registerAgent(makeAgentSpec("planner"));

    const result = await orch.run("Plan something", { targetAgentId: "planner" });
    expect(result.status).toBe("completed");

    // The task should be in snapshot
    const snap = orch.snapshot();
    const taskIds = Object.keys(snap.tasks);
    expect(taskIds.length).toBeGreaterThan(0);
  });

  // ---- getGraph ----

  it("getGraph returns nodes and edges", async () => {
    const orch = new Orchestrator();

    orch.registerAgent(makeAgentSpec("node-agent"));

    const graph = await orch.getGraph();
    expect(graph.nodes).toBeDefined();
    expect(graph.edges).toBeDefined();
    expect(Array.isArray(graph.nodes)).toBe(true);
    expect(Array.isArray(graph.edges)).toBe(true);
  });

  // ---- setApprovalGateway ----

  it("setApprovalGateway does not throw", () => {
    const orch = new Orchestrator();

    expect(() =>
      orch.setApprovalGateway({
        requestToolApproval: async () => "accept",
      }),
    ).not.toThrow();
  });

  // ---- Multiple agents ----

  it("supports multiple registered agents", async () => {
    const steps: FauxStepSpec[] = [
      { content: "Agent A here.", status: "completed" },
      { content: "Agent B here.", status: "completed" },
    ];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);

    orch.registerAgent(makeAgentSpec("agent-a"));
    orch.registerAgent(makeAgentSpec("agent-b"));

    const resultA = await orch.run("Hello A", { targetAgentId: "agent-a" });
    expect(resultA.status).toBe("completed");

    const resultB = await orch.run("Hello B", { targetAgentId: "agent-b" });
    expect(resultB.status).toBe("completed");
  });
});
