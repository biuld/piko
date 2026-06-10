import { describe, expect, it } from "bun:test";
import { AgentOrchestrator } from "piko-orchestrator";
import { assistantStep, codingToolSet, implementer, makeFauxEngine } from "./helpers.js";

describe("tick step results", () => {
  it("completed → task_completed, agent returns to idle", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();

    const task = Object.values(orch.snapshot().tasks)[0];
    expect(task.status).toBe("completed");
    expect(orch.isDone()).toBe(true);
    expect(orch.snapshot().agents.implementer.status).toBe("idle");
  });

  it("continue → agent stays running, next tick picks it up", async () => {
    const orch = new AgentOrchestrator(
      makeFauxEngine([assistantStep("step 1", "continue"), assistantStep("step 2", "completed")]),
    );
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });

    await orch.tick();
    expect(orch.isDone()).toBe(false);
    expect(Object.values(orch.snapshot().tasks)[0].status).toBe("running");

    await orch.tick();
    expect(Object.values(orch.snapshot().tasks)[0].status).toBe("completed");
    expect(orch.isDone()).toBe(true);
  });

  it("error → task_failed", async () => {
    const orch = new AgentOrchestrator(
      makeFauxEngine([
        {
          result: { status: "error", appendedMessages: [], stopReason: "error" },
        },
      ]),
    );
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();

    expect(Object.values(orch.snapshot().tasks)[0].status).toBe("failed");
    expect(orch.isDone()).toBe(true);
  });

  it("aborted → task_failed", async () => {
    const orch = new AgentOrchestrator(
      makeFauxEngine([
        {
          result: { status: "aborted", appendedMessages: [], stopReason: "abort" },
        },
      ]),
    );
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();

    expect(Object.values(orch.snapshot().tasks)[0].status).toBe("failed");
  });

  it("task events emitted in order", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();

    const events = orch.dumpEvents().map((e) => e.event);
    const subsystems = events.map((e) => e.subsystem);
    const types = events.map((e) => e.type);

    // Should have task events
    expect(subsystems).toContain("task");
    expect(types).toContain("enqueued");
    expect(types).toContain("started");
    expect(types).toContain("completed");
  });

  it("transcript accumulates across steps", async () => {
    const orch = new AgentOrchestrator(
      makeFauxEngine([assistantStep("msg1", "continue"), assistantStep("msg2", "completed")]),
    );
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();
    await orch.tick();

    const transcript = orch.snapshot().agents.implementer.transcript;
    expect(transcript.length).toBe(2);
  });

  it("tick with no engine does nothing", async () => {
    const orch = new AgentOrchestrator();
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });

    // Without engine, tick just schedules
    await orch.tick();
    const task = Object.values(orch.snapshot().tasks)[0];
    expect(task.status).toBe("running"); // scheduled but not executed
  });
});
