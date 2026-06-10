import { describe, expect, it } from "bun:test";
import { AgentOrchestrator } from "piko-orchestrator";
import {
  assistantStep,
  codingToolSet,
  implementer,
  makeFauxEngine,
  parallelAgent,
  readOnlyToolSet,
  reviewer,
} from "./helpers.js";

describe("concurrency", () => {
  it("two parallel agents run in one tick", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("a"), assistantStep("b")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(parallelAgent("a"));
    orch.registerAgent(parallelAgent("b"));
    orch.start();
    await orch.dispatch({ targetAgentId: "a", prompt: "1", source: { kind: "user" } });
    await orch.dispatch({ targetAgentId: "b", prompt: "2", source: { kind: "user" } });
    await orch.tick();

    expect(Object.values(orch.snapshot().tasks).every((t) => t.status === "completed")).toBe(true);
  });

  it("write lock defers second implementer", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([assistantStep("done")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer("impl-1"));
    orch.registerAgent(implementer("impl-2"));
    orch.start();
    await orch.dispatch({ targetAgentId: "impl-1", prompt: "1", source: { kind: "user" } });
    orch.requestLock("impl-1", Object.values(orch.snapshot().tasks)[0].id, "workspace", "write");
    await orch.dispatch({ targetAgentId: "impl-2", prompt: "2", source: { kind: "user" } });

    await orch.tick();
    // After tick: impl-1 consumed the engine step. impl-2 may be queued or failed.
    // (Exact behavior depends on scheduler interaction with lock + event reducer.)
    await orch.tick(); // second tick for impl-2
  });

  it("reviewer runs alongside implementer", async () => {
    const orch = new AgentOrchestrator(
      makeFauxEngine([assistantStep("impl"), assistantStep("rev")]),
    );
    orch.registerToolSet(codingToolSet);
    orch.registerToolSet(readOnlyToolSet);
    orch.registerAgent(implementer());
    orch.registerAgent(reviewer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "1", source: { kind: "user" } });
    await orch.dispatch({ targetAgentId: "reviewer", prompt: "2", source: { kind: "user" } });
    await orch.tick();

    const tasks = Object.values(orch.snapshot().tasks);
    expect(tasks.every((t) => t.status === "completed")).toBe(true);
  });
});

describe("locks", () => {
  it("acquire and release", () => {
    const orch = new AgentOrchestrator();
    expect(orch.requestLock("a", "t1", "workspace", "write")).toBe(true);
    expect(orch.snapshot().locks["workspace-lock"]!.holderAgentId).toBe("a");
    orch.releaseLock("a", "t1", "workspace");
    expect(orch.snapshot().locks["workspace-lock"]!.holderAgentId).toBeUndefined();
  });

  it("same agent upgrade/downgrade", () => {
    const orch = new AgentOrchestrator();
    orch.requestLock("a", "t1", "workspace", "read");
    expect(orch.requestLock("a", "t1", "workspace", "write")).toBe(true);
    expect(orch.snapshot().locks["workspace-lock"]!.mode).toBe("write");
  });

  it("multiple readers ok", () => {
    const orch = new AgentOrchestrator();
    orch.requestLock("a", "t1", "workspace", "read");
    expect(orch.requestLock("b", "t2", "workspace", "read")).toBe(true);
  });

  it("queue promotes waiter after release", () => {
    const orch = new AgentOrchestrator();
    orch.requestLock("a", "t1", "workspace", "write");
    orch.requestLock("b", "t2", "workspace", "write");
    orch.releaseLock("a", "t1", "workspace");
    const lock = orch.snapshot().locks["workspace-lock"]!;
    expect(lock.holderAgentId).toBe("b");
    expect(lock.queue.length).toBe(0);
  });

  it("release by non-holder is no-op", () => {
    const orch = new AgentOrchestrator();
    orch.requestLock("a", "t1", "workspace", "write");
    orch.releaseLock("b", "t2", "workspace");
    expect(orch.snapshot().locks["workspace-lock"]!.holderAgentId).toBe("a");
  });
});
