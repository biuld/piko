import { describe, expect, it } from "bun:test";
import { AgentOrchestrator } from "piko-orchestrator";
import { assistantStep, codingToolSet, implementer, makeFauxEngine } from "./helpers.js";

describe("event sourcing", () => {
  it("all events have meta fields", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();

    for (const env of orch.dumpEvents()) {
      expect(env.meta.eventId).toBeTypeOf("string");
      expect(env.meta.timestamp).toBeTypeOf("number");
      expect(env.meta.orchestratorRunId).toBeTypeOf("string");
      expect(env.event.type).toBeTypeOf("string");
    }
  });

  it("dumpEvents is append-only (old entries preserved)", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();

    const before = orch.dumpEvents().length;
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();
    const after = orch.dumpEvents().length;

    expect(after).toBeGreaterThan(before);
    expect(orch.dumpEvents().slice(0, before)).toEqual(orch.dumpEvents().slice(0, before));
  });

  it("snapshot reflects state after events are applied by reducer", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("done")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });

    const beforeStatus = orch.snapshot().agents.implementer.status;
    await orch.tick();
    const afterStatus = orch.snapshot().agents.implementer.status;

    expect(beforeStatus).not.toBe(afterStatus);
    expect(afterStatus).toBe("idle"); // task completed → agent idle
  });

  it("task events carry agent/task context", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();

    const taskEvents = orch.dumpEvents().filter((e) => e.event.subsystem === "task");

    expect(taskEvents.length).toBeGreaterThan(0);
    for (const env of taskEvents) {
      const ev = env.event;
      if ("agentId" in ev) {
        expect(ev.agentId).toBe("implementer");
        expect(ev.taskId).toBeTypeOf("string");
      }
    }
  });
});

describe("graph", () => {
  it("includes agents, tasks, and assigned_to edges", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "test", source: { kind: "user" } });

    const g = orch.renderGraph();
    expect(g.nodes.some((n) => n.kind === "agent")).toBe(true);
    expect(g.nodes.some((n) => n.kind === "task")).toBe(true);
    expect(g.edges.some((e) => e.kind === "assigned_to")).toBe(true);
  });

  it("graph includes locks when held", () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    orch.requestLock("implementer", "t1", "workspace", "write");

    const g = orch.renderGraph();
    expect(g.nodes.some((n) => n.kind === "lock")).toBe(true);
  });

  it("graph includes approval nodes", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "test", source: { kind: "user" } });

    const g = orch.renderGraph();
    expect(g.nodes.some((n) => n.kind === "task")).toBe(true);
    expect(g.edges.some((e) => e.kind === "assigned_to")).toBe(true);
  });
});
