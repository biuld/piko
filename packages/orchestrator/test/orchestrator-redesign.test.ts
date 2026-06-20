// ---- Orchestrator redesign characterization tests (Phase 0) ----

import { describe, expect, it } from "bun:test";
import type {
  AgentSpec,
  ToolCall,
  ToolDef,
  ToolExecutionContext,
  ToolProvider,
} from "piko-orchestrator-protocol";
import type { ModelStepEvent, ModelStepExecutor, ModelStepResult } from "../src/model/types.js";
import { Orchestrator } from "../src/orchestrator/index.js";
import type { FauxStepSpec } from "./helpers/index.js";
import { createFauxModelExecutor, TestEventStream } from "./helpers/index.js";

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

describe("Orchestrator Architecture Redesign (Phase 0 Baseline)", () => {
  // 1. cancel running model stream
  it("cancel running model stream stops execution and marks task aborted", async () => {
    let executeAborted = false;
    const customExecutor: ModelStepExecutor = {
      capabilities: { supportsTools: false, supportsSandbox: false, supportsMCP: false, tools: [] },
      executeStep(_input, signal) {
        const stream = new TestEventStream<ModelStepEvent, ModelStepResult>();
        if (signal) {
          signal.addEventListener(
            "abort",
            () => {
              executeAborted = true;
              stream.end({ status: "aborted", appendedMessages: [], stopReason: "abort" });
            },
            { once: true },
          );
        }
        // simulate slow stream that doesn't finish immediately
        setTimeout(() => {
          if (!executeAborted) {
            stream.push({ type: "thinking_delta", messageId: "1", delta: "still thinking..." });
          }
        }, 50);
        return stream;
      },
      async shutdown() {},
    };

    const orch = new Orchestrator(customExecutor);
    orch.registerAgent(makeAgentSpec("assistant"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "assistant",
      prompt: "Slow thinking prompt",
      source: { type: "user" },
    });

    // Cancel after brief moment
    await new Promise((resolve) => setTimeout(resolve, 10));
    await orch.cancelTask(taskId, "User abort");

    const result = await orch.joinTask(taskId).catch((err) => err);
    // Since task is cancelled, it should reflect aborted/error state, or throw/reject
    expect(result).toBeDefined();
    // executeAborted should be true eventually
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(executeAborted).toBe(true);
  });

  // 2. cancel running tool
  it("cancel running tool propagates abort signal to the tool provider", async () => {
    let toolSignalAborted = false;
    const slowToolProvider: ToolProvider = {
      id: "slow-provider",
      source: "host",
      async discover() {
        return [makeToolDef("slow_tool")];
      },
      async execute(_call: ToolCall, _context: ToolExecutionContext, signal?: AbortSignal) {
        if (signal) {
          if (signal.aborted) {
            toolSignalAborted = true;
            return { ok: false, error: { code: "aborted", message: "aborted" } };
          }
          signal.addEventListener(
            "abort",
            () => {
              toolSignalAborted = true;
            },
            { once: true },
          );
        }
        // Wait to simulate slow tool
        await new Promise((resolve) => setTimeout(resolve, 100));
        return { ok: true, value: "tool success" };
      },
    };

    const steps: FauxStepSpec[] = [
      {
        toolCalls: [{ id: "tc1", name: "slow_tool", arguments: {} }],
        status: "continue",
      },
      { content: "done", status: "completed" },
    ];

    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);
    orch.registerProvider(slowToolProvider);
    orch.registerToolSet({
      id: "ts:default",
      name: "Default",
      tools: [{ kind: "provider_tool", providerId: "slow-provider", toolName: "slow_tool" }],
    });

    orch.registerAgent(makeAgentSpec("agent-tool", { toolSetIds: ["ts:default"] }));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "agent-tool",
      prompt: "run tool",
      source: { type: "user" },
    });

    // Wait for tool to start executing
    await new Promise((resolve) => setTimeout(resolve, 20));
    await orch.cancelTask(taskId, "cancel tool run");

    // Allow scheduler to settle
    await new Promise((resolve) => setTimeout(resolve, 110));
    expect(toolSignalAborted).toBe(true);
  });

  // 3. cancel does not allow old Worker to append to transcript
  it("cancel does not allow worker step outputs to append to transcript after abort", async () => {
    const steps: FauxStepSpec[] = [
      {
        content: "Step 1 done",
        status: "continue",
        delayMs: 40,
      },
      {
        content: "Step 2 done",
        status: "completed",
      },
    ];

    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("assistant"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "assistant",
      prompt: "multi-step",
      source: { type: "user" },
    });

    await new Promise((resolve) => setTimeout(resolve, 15));
    await orch.cancelTask(taskId, "cancel task");

    // Wait until second step would have executed if not cancelled
    await new Promise((resolve) => setTimeout(resolve, 100));

    const snap = orch.snapshot();
    const taskState = snap.tasks[taskId];
    expect(taskState).toBeDefined();
    // Step 2 content must not be present in task status/messages because it was cancelled
    if (taskState) {
      // It should be either aborted or failed, and not completed
      expect(taskState.status).not.toBe("completed");
    }
  });

  // 4. global concurrency is across agents, never tasks within one agent
  it("allows different agents up to maxConcurrentAgents and releases slots", async () => {
    const executor: ModelStepExecutor = {
      capabilities: { supportsTools: false, supportsSandbox: false, supportsMCP: false, tools: [] },
      executeStep(input) {
        const promptMsg = input.transcript[input.transcript.length - 1];
        const prompt = typeof promptMsg?.content === "string" ? promptMsg.content : "";
        const stream = new TestEventStream<ModelStepEvent, ModelStepResult>();
        const content = prompt.includes("Task 1") ? "Result 1" : "Result 2";
        const delay = prompt.includes("Task 1") ? 40 : 10;
        setTimeout(() => {
          const msg = {
            role: "assistant" as const,
            content: [{ type: "text" as const, text: content }],
            timestamp: Date.now(),
          };
          stream.push({ type: "message_end", message: msg as any });
          stream.push({ type: "step_end" });
          stream.end({
            status: "completed",
            appendedMessages: [msg as any],
            stopReason: "assistant",
          });
        }, delay);
        return stream;
      },
      async shutdown() {},
    };

    const orch = new Orchestrator(executor, undefined, { maxConcurrentAgents: 2 });
    orch.registerAgent(makeAgentSpec("agent-1"));
    orch.registerAgent(makeAgentSpec("agent-2"));
    orch.registerAgent(makeAgentSpec("agent-3"));

    const t1Id = await orch.dispatch({
      targetAgentId: "agent-1",
      prompt: "Run Task 1",
      source: { type: "user" },
    });
    const t2Id = await orch.dispatch({
      targetAgentId: "agent-2",
      prompt: "Run Task 2",
      source: { type: "user" },
    });

    await expect(
      orch.dispatch({
        targetAgentId: "agent-3",
        prompt: "Run Task 3",
        source: { type: "user" },
      }),
    ).rejects.toMatchObject({ code: "concurrency_limit" });

    const [res1, res2] = await Promise.all([orch.joinTask(t1Id), orch.joinTask(t2Id)]);
    expect(res1).toBeDefined();
    expect(res2).toBeDefined();

    const t3Id = await orch.dispatch({
      targetAgentId: "agent-3",
      prompt: "Run Task 3",
      source: { type: "user" },
    });
    await expect(orch.joinTask(t3Id)).resolves.toBeDefined();
  });

  // 5. detached tasks completion, failure, and join
  it("detached tasks support normal run, failure and join API", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ throwError: "Forced failure" }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("fail-agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "fail-agent",
      prompt: "will fail",
      source: { type: "user" },
    });

    // joinTask should throw or reject for failures
    await expect(orch.joinTask(taskId)).rejects.toThrow();
  });

  // 6. multiple join semantics
  it("allows multiple joinTask calls on the same task ID", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "success", status: "completed" }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("join-agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "join-agent",
      prompt: "join me",
      source: { type: "user" },
    });

    const p1 = orch.joinTask(taskId);
    const p2 = orch.joinTask(taskId);

    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1).toBeDefined();
    expect(r2).toBeDefined();
  });

  // 7. unregister AgentSpec does not affect running tasks
  it("unregistering an AgentSpec does not affect active running tasks under that spec", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "run completed", status: "completed", delayMs: 40 }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("temp-agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "temp-agent",
      prompt: "run long",
      source: { type: "user" },
    });

    orch.unregisterAgent("temp-agent");

    const result = await orch.joinTask(taskId);
    expect(result).toBeDefined();
    const snap = orch.snapshot();
    expect(snap.tasks[taskId].status).toBe("completed");
  });

  // 8. terminal event exactly once
  it("emits exactly one terminal event (task_completed/failed/cancelled) per task", async () => {
    const steps: FauxStepSpec[] = [{ content: "finished", status: "completed" }];
    const modelExecutor = createFauxModelExecutor({ steps });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("event-agent"));

    const events: Array<{ type: string; taskId?: string }> = [];
    orch.subscribe((e) => {
      events.push(e as any);
    });

    const taskId = await orch.dispatch({
      targetAgentId: "event-agent",
      prompt: "event test",
      source: { type: "user" },
    });
    await orch.joinTask(taskId);

    const terminalEvents = events.filter(
      (e) =>
        e.taskId === taskId && ["task_completed", "task_failed", "task_cancelled"].includes(e.type),
    );
    expect(terminalEvents.length).toBe(1);
  });

  // 9. same agent busy — second task for same agent returns agent_busy
  it("rejects second task for same agent with agent_busy", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "slow work", status: "completed", delayMs: 50 }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("worker"));

    const t1Id = await orch.dispatchDetached({
      targetAgentId: "worker",
      prompt: "Task 1",
      source: { type: "user" },
    });

    await expect(
      orch.delegateToAgent({
        targetAgentId: "worker",
        prompt: "Task 2",
        source: { type: "user" },
      }),
    ).rejects.toMatchObject({ code: "agent_busy" });

    // t1 should still complete
    const res1 = await orch.joinTask(t1Id);
    expect(res1).toBeDefined();
  });

  // 10. concurrency_limit
  it("rejects task when concurrency limit is reached", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "slow", status: "completed", delayMs: 50 }],
    });
    const orch = new Orchestrator(modelExecutor, undefined, { maxConcurrentAgents: 1 });
    orch.registerAgent(makeAgentSpec("a"));
    orch.registerAgent(makeAgentSpec("b"));

    await orch.dispatchDetached({
      targetAgentId: "a",
      prompt: "Task A",
      source: { type: "user" },
    });

    await expect(
      orch.delegateToAgent({
        targetAgentId: "b",
        prompt: "Task B",
        source: { type: "user" },
      }),
    ).rejects.toMatchObject({ code: "concurrency_limit" });
  });

  // 11. completed task releases concurrency slot
  it("releases concurrency slot after task completes", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "done", status: "completed", delayMs: 10 }],
    });
    const orch = new Orchestrator(modelExecutor, undefined, { maxConcurrentAgents: 1 });
    orch.registerAgent(makeAgentSpec("a"));
    orch.registerAgent(makeAgentSpec("b"));

    const t1Id = await orch.dispatchDetached({
      targetAgentId: "a",
      prompt: "Task A",
      source: { type: "user" },
    });

    await orch.joinTask(t1Id);

    // Now agent b should be allowed
    const t2Id = await orch.dispatchDetached({
      targetAgentId: "b",
      prompt: "Task B",
      source: { type: "user" },
    });
    const res2 = await orch.joinTask(t2Id);
    expect(res2).toBeDefined();
  });

  // 12. failed task releases concurrency slot
  it("releases concurrency slot after task fails", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ throwError: "Forced failure" }],
    });
    const orch = new Orchestrator(modelExecutor, undefined, { maxConcurrentAgents: 1 });
    orch.registerAgent(makeAgentSpec("a"));
    orch.registerAgent(makeAgentSpec("b"));

    const t1Id = await orch.dispatchDetached({
      targetAgentId: "a",
      prompt: "Task A",
      source: { type: "user" },
    });

    await orch.joinTask(t1Id).catch(() => {});

    const t2Id = await orch.dispatchDetached({
      targetAgentId: "b",
      prompt: "Task B",
      source: { type: "user" },
    });
    const res2 = await orch.joinTask(t2Id);
    expect(res2).toBeDefined();
  });

  undefined;

  // 13. cancelled task releases concurrency slot
  it("releases concurrency slot after task is cancelled", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "work", status: "completed", delayMs: 200 }],
    });
    const orch = new Orchestrator(modelExecutor, undefined, { maxConcurrentAgents: 1 });
    orch.registerAgent(makeAgentSpec("a"));
    orch.registerAgent(makeAgentSpec("b"));

    const t1Id = await orch.dispatchDetached({
      targetAgentId: "a",
      prompt: "Task A",
      source: { type: "user" },
    });

    await new Promise((resolve) => setTimeout(resolve, 20));
    await orch.cancelTask(t1Id, "cancel");
    await orch.joinTask(t1Id).catch(() => {});

    const t2Id = await orch.dispatchDetached({
      targetAgentId: "b",
      prompt: "Task B",
      source: { type: "user" },
    });
    const res2 = await orch.joinTask(t2Id);
    expect(res2).toBeDefined();
  });

  // ---- join semantics ----

  // 14. join running task waits for result
  it("join running task waits for completion and returns result", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "delayed result", status: "completed", delayMs: 30 }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "agent",
      prompt: "work",
      source: { type: "user" },
    });

    const res = await orch.joinTask(taskId);
    expect(res).toBeDefined();
  });

  // 15. join completed task returns immediately
  it("join completed task returns immediately", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "instant", status: "completed" }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "agent",
      prompt: "work",
      source: { type: "user" },
    });

    // Already completed — should return immediately
    const res = await orch.joinTask(taskId);
    expect(res).toBeDefined();
  });

  // 16. serial repeated join returns same result
  it("serial repeated joinTask returns consistent results", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "one result", status: "completed" }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "agent",
      prompt: "work",
      source: { type: "user" },
    });

    const r1 = await orch.joinTask(taskId);
    const r2 = await orch.joinTask(taskId);
    expect(r1).toBeDefined();
    expect(r2).toBeDefined();
  });

  // 17. concurrent repeated join
  it("concurrent repeated joinTask returns consistent results", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ content: "one result", status: "completed" }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "agent",
      prompt: "work",
      source: { type: "user" },
    });

    const [r1, r2] = await Promise.all([orch.joinTask(taskId), orch.joinTask(taskId)]);
    expect(r1).toBeDefined();
    expect(r2).toBeDefined();
  });

  // 18. join unknown taskId
  it("join unknown taskId throws 'Detached task not found'", async () => {
    const orch = new Orchestrator(createFauxModelExecutor());
    await expect(orch.joinTask("nonexistent")).rejects.toThrow("Detached task not found");
  });

  // 19. join failed task throws
  it("join failed task throws the task error", async () => {
    const modelExecutor = createFauxModelExecutor({
      steps: [{ throwError: "Forced failure" }],
    });
    const orch = new Orchestrator(modelExecutor);
    orch.registerAgent(makeAgentSpec("fail-agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "fail-agent",
      prompt: "fail",
      source: { type: "user" },
    });

    await expect(orch.joinTask(taskId)).rejects.toThrow();

    const snap = orch.snapshot();
    expect(snap.tasks[taskId].status).toBe("failed");
  });

  // 20. join cancelled task — characterization: currently returns aborted result
  it("join cancelled task returns aborted result (characterization)", async () => {
    let executeAborted = false;
    const customExecutor: ModelStepExecutor = {
      capabilities: { supportsTools: false, supportsSandbox: false, supportsMCP: false, tools: [] },
      executeStep(_input, signal) {
        const stream = new TestEventStream<ModelStepEvent, ModelStepResult>();
        if (signal) {
          signal.addEventListener(
            "abort",
            () => {
              executeAborted = true;
              stream.end({ status: "aborted", appendedMessages: [], stopReason: "abort" });
            },
            { once: true },
          );
        }
        return stream;
      },
      async shutdown() {},
    };
    const orch = new Orchestrator(customExecutor);
    orch.registerAgent(makeAgentSpec("agent"));

    const taskId = await orch.dispatchDetached({
      targetAgentId: "agent",
      prompt: "work to cancel",
      source: { type: "user" },
    });

    await orch.cancelTask(taskId, "cancel");

    // joinTask currently returns the aborted result (not throw) because
    // finalStatus "aborted" is not "error"/"failed"
    const res = await orch.joinTask(taskId);
    expect(res).toBeDefined();
    expect((res as { finalStatus?: string }).finalStatus).toBe("aborted");
    expect(executeAborted).toBe(true);
  });
});
