import { describe, expect, it } from "bun:test";
import type { AgentTask } from "piko-orchestrator-protocol";
import { Orchestrator } from "../../src/orchestrator/orchestrator.js";
import { createFauxModelExecutor } from "../helpers/index.js";

describe("AgentActor Lifecycle and Concurrency", () => {
  it("P0: cancel waits for worker to settle, stops actor, and finalizes once", async () => {
    // 1. Setup orchestrator with a slow task step
    const executor = createFauxModelExecutor({
      steps: [
        {
          content: "Waiting...",
          delayMs: 100, // Make it slow so we can cancel it mid-run
          status: "continue",
        },
      ],
    });

    const orch = new Orchestrator(executor);
    const spec = {
      id: "test-agent",
      name: "Test Agent",
      role: "test",
      systemPrompt: "test",
      toolSetIds: [],
    };
    orch.registerAgent(spec);

    const task: AgentTask = {
      id: "task-1",
      targetAgentId: "test-agent",
      prompt: "Go",
      source: { type: "user" },
    };

    // 2. Dispatch task
    const taskId = await orch.dispatch(task);
    const run = orch.runs.get(taskId)!;
    expect(run).toBeDefined();

    // Wait slightly to let it start running
    await new Promise((resolve) => setTimeout(resolve, 20));

    // Actor should be spawned in the system
    expect(orch.system.getActorIds()).toContain(`agent:test-agent:task:${taskId}`);

    // 3. Cancel task
    const cancelPromise = orch.cancelTask(taskId, "user aborted");

    // Check status immediately
    expect(run.status).toBe("cancelling");

    // Await cancellation to resolve
    await cancelPromise;

    // Await result promise to settle
    const res = await run.resultPromise;
    expect(res.finalStatus).toBe("aborted");

    // Actor should be stopped and removed from the system
    expect(orch.system.getActorIds()).not.toContain(`agent:test-agent:task:${taskId}`);
  });

  it("P0: rejects duplicate task IDs", async () => {
    const executor = createFauxModelExecutor({
      steps: [{ content: "Done", status: "completed" }],
    });
    const orch = new Orchestrator(executor);
    const spec = {
      id: "test-agent",
      name: "Test Agent",
      role: "test",
      systemPrompt: "test",
      toolSetIds: [],
    };
    orch.registerAgent(spec);

    const task1: AgentTask = {
      id: "dup-task",
      targetAgentId: "test-agent",
      prompt: "Go 1",
      source: { type: "user" },
    };

    await orch.dispatch(task1);

    const task2: AgentTask = {
      id: "dup-task",
      targetAgentId: "test-agent",
      prompt: "Go 2",
      source: { type: "user" },
    };

    expect(orch.dispatch(task2)).rejects.toThrow("Duplicate task ID: dup-task");
  });

  it("P1: rejects duplicate task IDs even after RunHandle cleanup", async () => {
    const executor = createFauxModelExecutor({
      steps: [{ content: "Done", status: "completed" }],
    });
    const orch = new Orchestrator(executor);
    const spec = {
      id: "test-agent",
      name: "Test Agent",
      role: "test",
      systemPrompt: "test",
      toolSetIds: [],
    };
    orch.registerAgent(spec);

    // 1. Dispatch first task
    const task1: AgentTask = {
      id: "dup-task-cleanup",
      targetAgentId: "test-agent",
      prompt: "Go 1",
      source: { type: "user" },
    };
    await orch.dispatch(task1);

    // Verify it's in runs
    expect(orch.runs.has("dup-task-cleanup")).toBe(true);

    // 2. Fill the runs list with dummy runs to trigger eviction (need >= 100 runs)
    for (let i = 0; i < 101; i++) {
      const dummyTask: AgentTask = {
        id: `dummy-task-${i}`,
        targetAgentId: "test-agent",
        prompt: `Dummy ${i}`,
        source: { type: "user" },
      };
      await orch.dispatch(dummyTask);
      const run = orch.runs.get(`dummy-task-${i}`)!;
      await run.resultPromise;
    }

    // Now trigger another dispatch to force the cleanup if not already done,
    // though the first task is likely already evicted.
    // Verify that "dup-task-cleanup" has been evicted from orch.runs
    expect(orch.runs.has("dup-task-cleanup")).toBe(false);

    // 3. Dispatch a task with the duplicate ID again and expect it to be rejected
    const task2: AgentTask = {
      id: "dup-task-cleanup",
      targetAgentId: "test-agent",
      prompt: "Go 2",
      source: { type: "user" },
    };

    expect(orch.dispatch(task2)).rejects.toThrow("Duplicate task ID: dup-task-cleanup");
  });

  it("retains completed detached tasks until they are joined", async () => {
    const executor = createFauxModelExecutor({
      steps: [{ content: "Done", status: "completed" }],
    });
    const orch = new Orchestrator(executor);
    orch.registerAgent({
      id: "test-agent",
      name: "Test Agent",
      role: "test",
      systemPrompt: "test",
      toolSetIds: [],
    });

    const detachedTaskId = await orch.dispatchDetached({
      id: "detached-before-cleanup",
      targetAgentId: "test-agent",
      prompt: "Detached",
      source: { type: "user" },
    });
    await orch.runs.get(detachedTaskId)!.resultPromise;

    for (let i = 0; i < 101; i++) {
      const taskId = await orch.dispatch({
        id: `cleanup-task-${i}`,
        targetAgentId: "test-agent",
        prompt: `Cleanup ${i}`,
        source: { type: "user" },
      });
      await orch.runs.get(taskId)!.resultPromise;
    }

    expect(orch.runs.has(detachedTaskId)).toBe(true);
    await expect(orch.joinTask(detachedTaskId)).resolves.toBeDefined();
  });

  it("rejects cancellation routed to an actor that does not own the task", async () => {
    const executor = createFauxModelExecutor({
      steps: [{ content: "Waiting", delayMs: 50, status: "completed" }],
    });
    const orch = new Orchestrator(executor);
    orch.registerAgent({
      id: "test-agent",
      name: "Test Agent",
      role: "test",
      systemPrompt: "test",
      toolSetIds: [],
    });

    const taskId = await orch.dispatch({
      id: "owned-task",
      targetAgentId: "test-agent",
      prompt: "Run",
      source: { type: "user" },
    });
    const actorId = orch.runs.get(taskId)!.actorId;

    await expect(
      orch.system.ask(actorId, { type: "cancel", taskId: "foreign-task" }),
    ).rejects.toThrow("is not owned by actor");
    expect(orch.snapshot().tasks["foreign-task"]).toBeUndefined();

    await orch.runs.get(taskId)!.resultPromise;
  });

  it("P1: pre-aborted task does not execute and returns aborted status", async () => {
    const executor = createFauxModelExecutor({
      steps: [{ content: "Done", status: "completed" }],
    });
    const orch = new Orchestrator(executor);
    const spec = {
      id: "test-agent",
      name: "Test Agent",
      role: "test",
      systemPrompt: "test",
      toolSetIds: [],
    };
    orch.registerAgent(spec);

    const controller = new AbortController();
    controller.abort();

    const res = await orch.run("Hello", {
      targetAgentId: "test-agent",
      signal: controller.signal,
    });

    expect(res.status).toBe("aborted");
    // Verify no actor was spawned for this run (since we aborted early)
    expect(orch.system.getActorIds().some((id) => id.startsWith("agent:test-agent"))).toBe(false);
  });

  it("P1: agent status/activeTaskId projection is correct under concurrent tasks", async () => {
    // 1. Setup two concurrent tasks
    const executor = createFauxModelExecutor({
      steps: [
        { content: "Step 1", delayMs: 40, status: "completed" },
        { content: "Step 2", delayMs: 80, status: "completed" },
      ],
    });
    const orch = new Orchestrator(executor);
    const spec = {
      id: "test-agent",
      name: "Test Agent",
      role: "test",
      systemPrompt: "test",
      toolSetIds: [],
    };
    orch.registerAgent(spec);

    // Start both
    const p1 = orch.run("Task 1", { targetAgentId: "test-agent" });
    await new Promise((resolve) => setTimeout(resolve, 10));
    const p2 = orch.run("Task 2", { targetAgentId: "test-agent" });

    // Wait slightly
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Agent status should be running
    let snap = orch.snapshot();
    expect(snap.agents["test-agent"].status).toBe("running");

    // Wait for the first task to finish (e.g. at 50ms)
    await p1;

    // Agent status should STILL be running because task 2 is still running!
    snap = orch.snapshot();
    expect(snap.agents["test-agent"].status).toBe("running");
    expect(snap.agents["test-agent"].activeTaskId).toBeDefined();

    // Wait for the second task to finish
    await p2;

    // Agent status should now be idle
    snap = orch.snapshot();
    expect(snap.agents["test-agent"].status).toBe("idle");
    expect(snap.agents["test-agent"].activeTaskId).toBeUndefined();
  });

  it("P1: cancels while waiting for approval without hanging", async () => {
    // 1. Setup a custom approval gateway that delays
    let approvalCount = 0;
    const approvalGateway = {
      requestToolApproval: async () => {
        approvalCount++;
        await new Promise((resolve) => setTimeout(resolve, 500));
        return "accept" as const;
      },
    };

    const executor = createFauxModelExecutor({
      steps: [
        {
          toolCalls: [{ id: "tc1", name: "test_tool", arguments: {} }],
          status: "continue",
        },
        { content: "Done", status: "completed" },
      ],
    });

    const orch = new Orchestrator(executor);
    const spec = {
      id: "test-agent",
      name: "Test Agent",
      role: "test",
      systemPrompt: "test",
      toolSetIds: ["ts:custom"],
    };
    orch.registerAgent(spec);
    orch.setApprovalGateway(approvalGateway);

    orch.toolRegistry.registerToolSet({
      id: "ts:custom",
      name: "Custom",
      tools: [{ kind: "provider_tool", providerId: "mock-prov", toolName: "test_tool" }],
    });

    // Register a tool that requires approval
    orch.toolRegistry.registerProvider({
      id: "mock-prov",
      source: "workspace",
      discover: async () => [
        {
          name: "test_tool",
          description: "test",
          approval: "always",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "host", target: "test" },
        },
      ],
      execute: async () => ({ ok: true, value: "tool success" }),
    });

    const task: AgentTask = {
      id: "approval-cancel-task",
      targetAgentId: "test-agent",
      prompt: "Call tool",
      source: { type: "user" },
    };

    const taskId = await orch.dispatch(task);
    const runHandle = orch.runs.get(taskId)!;

    // Wait slightly to let it request approval
    await new Promise((resolve) => setTimeout(resolve, 50));

    // Expect approval to be requested
    expect(approvalCount).toBe(1);

    // Now cancel the task while it's waiting for approval
    await orch.cancelTask(task.id!, "aborted during approval");

    const result = await runHandle.resultPromise;
    expect(result.finalStatus).toBe("aborted");
  });
});
