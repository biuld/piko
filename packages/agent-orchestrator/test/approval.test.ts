import { describe, expect, it } from "bun:test";
import { AgentOrchestrator } from "piko-agent-orchestrator";
import {
  approvalStep,
  assistantStep,
  codingToolSet,
  implementer,
  makeFauxEngine,
} from "./helpers.js";

describe("approval flow", () => {
  it("awaiting_approval blocks agent, tick skips it until resolved", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([approvalStep(), assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });

    await orch.tick();
    expect(orch.getPendingApprovals().length).toBe(1);
    expect(Object.values(orch.snapshot().tasks)[0].status).toBe("running"); // blocked, not terminal

    // Second tick skips blocked agent
    await orch.tick();
    expect(orch.getPendingApprovals().length).toBe(1);

    // Resolve
    await orch.resolveApproval("implementer", "tc1", "accept");
    expect(orch.getPendingApprovals().length).toBe(0);

    // Now tick clears the task
    await orch.tick();
    expect(Object.values(orch.snapshot().tasks)[0].status).toBe("completed");
  });

  it("decline fails the task", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([approvalStep()]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();
    await orch.resolveApproval("implementer", "tc1", "decline");
    expect(Object.values(orch.snapshot().tasks)[0].status).toBe("failed");
  });

  it("resolveApproval calls engine.resolveApproval", async () => {
    let called = false;
    const engine = makeFauxEngine([approvalStep()], async (_r) => {
      called = true;
      return { status: "completed", appendedMessages: [], stopReason: "assistant" };
    });
    const orch = new AgentOrchestrator(engine);
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();
    await orch.resolveApproval("implementer", "tc1", "accept");
    expect(called).toBe(true);
  });

  it("resolveApproval throws for unknown approval", async () => {
    const orch = new AgentOrchestrator();
    await expect(orch.resolveApproval("x", "y", "accept")).rejects.toThrow("No matching approval");
  });

  it("two consecutive approvals in same task", async () => {
    let resolveCount = 0;
    const engine = makeFauxEngine(
      [approvalStep("shell"), approvalStep("apply_patch"), assistantStep("done")],
      async (_r) => {
        resolveCount++;
        return { status: "continue", appendedMessages: [], stopReason: "tool" };
      },
    );
    const orch = new AgentOrchestrator(engine);
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });

    await orch.tick();
    expect(orch.getPendingApprovals().length).toBe(1);
    await orch.resolveApproval("implementer", "tc1", "accept");
    expect(resolveCount).toBe(1);

    await orch.tick();
    expect(orch.getPendingApprovals().length).toBe(1);
    await orch.resolveApproval("implementer", "tc1", "accept");
    expect(resolveCount).toBe(2);

    await orch.tick();
    expect(Object.values(orch.snapshot().tasks)[0].status).toBe("completed");
  });

  it("approval_requested and approval_resolved events emitted", async () => {
    const orch = new AgentOrchestrator(makeFauxEngine([approvalStep(), assistantStep("ok")]));
    orch.registerToolSet(codingToolSet);
    orch.registerAgent(implementer());
    orch.start();
    await orch.dispatch({ targetAgentId: "implementer", prompt: "x", source: { kind: "user" } });
    await orch.tick();

    const types = orch.dumpEvents().map((e) => e.event.type);
    expect(types).toContain("approval_requested");

    await orch.resolveApproval("implementer", "tc1", "accept");
    expect(orch.dumpEvents().map((e) => e.event.type)).toContain("approval_resolved");
  });
});
