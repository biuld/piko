import { describe, expect, it } from "bun:test";
import { AgentOrchestrator } from "piko-agent-orchestrator";
import {
  approvalStep,
  assistantStep,
  codingToolSet,
  collect,
  implementer,
  makeFauxEngine,
} from "./helpers.js";

describe("lifecycle", () => {
  it("initial state", () => {
    const orch = new AgentOrchestrator();
    expect(orch.snapshot().status).toBe("idle");
    // No tasks → nothing to be "done" with
    expect(orch.isDone()).toBe(false);
  });

  it("start emits orchestrator_started", () => {
    const orch = new AgentOrchestrator();
    const { events } = collect(orch);
    orch.start();
    expect(events.map((e) => e.event.type)).toContain("orchestrator_started");
    expect(orch.snapshot().status).toBe("running");
  });

  it("double start is idempotent", () => {
    const orch = new AgentOrchestrator();
    orch.start();
    orch.start();
    expect(orch.dumpEvents().filter((e) => e.event.type === "orchestrator_started").length).toBe(1);
  });

  it("stop clears approvals and marks stopped", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([approvalStep()]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();

    expect(orch.getPendingApprovals().length).toBe(1);
    await orch.stop();

    expect(orch.getPendingApprovals().length).toBe(0);
    expect(orch.snapshot().status).toBe("stopped");
  });

  it("register/unregister tool sets", () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    expect(orch.snapshot().toolSets["builtin:core-coding"]).toBeDefined();
    orch.unregisterToolSet("builtin:core-coding");
    expect(orch.snapshot().toolSets["builtin:core-coding"]).toBeUndefined();
  });

  it("register/unregister agents", () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    expect(orch.snapshot().agents.implementer).toBeDefined();
    orch.unregisterAgent("implementer");
    expect(orch.snapshot().agents.implementer).toBeUndefined();
  });

  it("rejects agent with missing tool set", () => {
    const orch = new AgentOrchestrator();
    expect(() => orch.registerAgent(implementer())).toThrow("unknown ToolSet");
  });
});

describe("run()", () => {
  it("full lifecycle with single step", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("done")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    const result = await orch.run("hello");
    expect(result.status).toBe("completed");
    expect(result.messages.length).toBe(1);
  });

  it("abort via signal", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("x"), assistantStep("y")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    const ctrl = new AbortController();
    ctrl.abort();
    const result = await orch.run("x", { signal: ctrl.signal });
    expect(result.status).toBe("aborted");
  });

  it("maxSteps reached", async () => {
    const steps = new Array(10).fill(assistantStep("x", "continue"));
    const orch = new AgentOrchestrator(makeFauxEngine(steps), {
      model: {} as never,
      provider: {},
      settings: { maxSteps: 3, allowToolCalls: true, allowApprovals: true },
    });
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    const result = await orch.run("x");
    expect(result.status).toBe("max_steps");
  });

  it("throws if agent not registered", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("x")]));
    await expect(orch.run("x")).rejects.toThrow("not registered");
  });

  it("throws if no engine configured", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    await expect(orch.run("x")).rejects.toThrow("No engine");
  });
});

describe("task management", () => {
  it("blockTask", () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    const taskId = Object.values(orch.snapshot().tasks)[0].id;
    orch.blockTask(taskId, "blocked");
    expect(orch.snapshot().tasks[taskId].status).toBe("blocked");
  });

  it("isDone is false when tasks are queued", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("a", "continue")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "1", source: { kind: "user" } });

    await orch.tick();
    // continue status → not done yet
    expect(orch.isDone()).toBe(false);
  });
});
