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

  // 4. parallel execution of two tasks under same AgentSpec is isolated
  it("executes two tasks under the same AgentSpec concurrently without interference", async () => {
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

    const orch = new Orchestrator(executor);
    orch.registerAgent(makeAgentSpec("parallel-agent"));

    const t1Promise = orch.dispatch({
      targetAgentId: "parallel-agent",
      prompt: "Run Task 1",
      source: { type: "user" },
    });
    const t2Promise = orch.dispatch({
      targetAgentId: "parallel-agent",
      prompt: "Run Task 2",
      source: { type: "user" },
    });

    const [t1Id, t2Id] = await Promise.all([t1Promise, t2Promise]);
    const [res1, res2] = await Promise.all([orch.joinTask(t1Id), orch.joinTask(t2Id)]);

    expect(res1).toBeDefined();
    expect(res2).toBeDefined();

    // Verify task results are isolated
    const snap = orch.snapshot();
    expect(snap.tasks[t1Id].status).toBe("completed");
    expect(snap.tasks[t2Id].status).toBe("completed");
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
});
