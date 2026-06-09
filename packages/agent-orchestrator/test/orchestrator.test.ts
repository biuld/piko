// Integration: full run() lifecycle with watch + dependency
import { describe, expect, it } from "bun:test";
import { AgentOrchestrator } from "piko-agent-orchestrator";
import { assistantStep, codingToolSet, implementer, makeFauxEngine } from "./helpers.js";

describe("integration: run() with dependency watch", () => {
  it("parent task completes when child finishes", async () => {
    const orch = new AgentOrchestrator(
      makeFauxEngine([assistantStep("child"), assistantStep("parent after child done")]),
    );
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer("parent"));
    orch.registerAgent(implementer("child"));
    orch.start();

    const parentId = await orch.dispatch({
      targetAgentId: "parent",
      prompt: "parent task",
      source: { kind: "user" },
    });
    await orch.dispatch({
      targetAgentId: "child",
      prompt: "child task",
      source: { kind: "user" },
      parentTaskId: parentId,
    });

    // Tick: child runs first (parent blocked by write lock)
    await orch.tick();
    await orch.tick();

    const events = orch.dumpEvents().map((e) => e.event.type);
    expect(events).toContain("watch_triggered");
  });
});
