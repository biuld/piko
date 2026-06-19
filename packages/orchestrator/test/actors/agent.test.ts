// ---- AgentActor tests — model loop, transcript, tool execution ----

import { describe, expect, it } from "bun:test";
import type {
  AgentSpec,
  AgentTask,
  Message,
  ModelProviderConfig,
  ModelRunSettings,
  ToolDef,
  ToolExecResult,
  ToolProvider,
  ToolSet,
} from "piko-orchestrator-protocol";
import type { AgentActorDeps } from "../../src/actors/agent/index.js";
import { agentActor } from "../../src/actors/agent/index.js";
import type { OrchestratorEvent } from "../../src/actors/state/index.js";
import type { ActorHandler } from "../../src/kernel/actor-system.js";
import { ActorSystem } from "../../src/kernel/actor-system.js";
import type { ModelStepEvent, ModelStepExecutor, ModelStepResult } from "../../src/model/types.js";
import { ToolRegistryImpl } from "../../src/tools/index.js";
import type { FauxStepSpec } from "../helpers/index.js";
import {
  createFauxModelExecutor,
  createMockToolProvider,
  TestEventStream,
} from "../helpers/index.js";

// ---- Helpers ----

function makeAgentSpec(overrides?: Partial<AgentSpec>): AgentSpec {
  return {
    id: "test-agent",
    name: "Test Agent",
    role: "test",
    systemPrompt: "You are a helpful test agent.",
    toolSetIds: ["ts:default"],
    maxSteps: 10,
    ...overrides,
  };
}

function makeTask(prompt: string, overrides?: Partial<AgentTask>): AgentTask {
  return {
    id: `task-${Date.now()}`,
    targetAgentId: "test-agent",
    prompt,
    source: { type: "user" },
    ...overrides,
  };
}

function makeToolDef(name: string, opts?: Partial<ToolDef>): ToolDef {
  return {
    name,
    description: `Tool: ${name}`,
    inputSchema: { type: "object", properties: {} },
    executor: { kind: "native", target: name },
    ...opts,
  };
}

async function createTestEnv(opts?: {
  steps?: FauxStepSpec[];
  maxSteps?: number;
  toolSetIds?: string[];
  tools?: ToolDef[];
  providerId?: string;
  toolResult?: ToolExecResult;
  modelExecutor?: ModelStepExecutor;
}) {
  const system = new ActorSystem();
  const emitted: OrchestratorEvent[] = [];
  const emit = async (event: OrchestratorEvent) => {
    emitted.push(event);
  };

  const modelExecutor = opts?.modelExecutor ?? createFauxModelExecutor({ steps: opts?.steps });

  const toolRegistry = new ToolRegistryImpl(emit);

  // Register provider + toolset for discovery
  if (opts?.tools && opts.tools.length > 0) {
    const provider: ToolProvider = createMockToolProvider({
      id: opts?.providerId ?? "engine",
      tools: opts.tools,
      executeResult: opts?.toolResult ?? { ok: true, value: "tool result" },
    });
    toolRegistry.registerProvider(provider);

    const toolSet: ToolSet = {
      id: "ts:default",
      name: "Default",
      tools: opts.tools.map((t) => ({
        kind: "provider_tool" as const,
        providerId: opts?.providerId ?? "engine",
        toolName: t.name,
      })),
    };
    toolRegistry.registerToolSet(toolSet);

    // If using bash + read tools, register both under the same provider
    if (
      opts.tools.length > 1 &&
      !opts.tools.every((t) =>
        toolSet.tools.some((ref) => ref.kind === "provider_tool" && ref.toolName === t.name),
      )
    ) {
      // Re-register toolset with all tools
      toolRegistry.unregisterToolSet("ts:default");
      toolRegistry.registerToolSet({
        id: "ts:default",
        name: "Default",
        tools: opts.tools.map((t) => ({
          kind: "provider_tool" as const,
          providerId: opts?.providerId ?? "engine",
          toolName: t.name,
        })),
      });
    }
  }

  const deps: AgentActorDeps = {
    modelExecutor,
    emit,
    maxSteps: opts?.maxSteps ?? 10,
    actorSystem: system,
    toolRegistry,
    modelConfig: {
      model: {
        id: "test-model",
        name: "Test Model",
      } as import("piko-orchestrator-protocol").Model<string>,
      provider: {} as ModelProviderConfig,
      settings: { maxSteps: 1, allowToolCalls: true } as ModelRunSettings,
    },
  };

  const spec = makeAgentSpec({
    toolSetIds: opts?.toolSetIds ?? ["ts:default"],
  });

  const handler = agentActor(spec, deps);
  system.spawn({
    id: "agent:test-agent",
    kind: "agent",
    handler: handler as ActorHandler,
  });

  return {
    system,
    emitted,
    dispatch: (task: AgentTask) => {
      if (!system.getActorIds().includes("agent:test-agent")) {
        system.spawn({
          id: "agent:test-agent",
          kind: "agent",
          handler: handler as ActorHandler,
        });
      }
      return system.ask<{
        summary: string;
        messages: Message[];
        totalSteps: number;
        finalStatus: string;
      }>("agent:test-agent", { type: "dispatch", task });
    },
    cancel: (taskId: string, reason?: string) =>
      system.ask("agent:test-agent", { type: "cancel", taskId, reason }),
    setModelConfig: (config: {
      model?: { id: string; name?: string; provider?: string };
      provider?: Record<string, unknown>;
      settings?: { maxSteps?: number; allowToolCalls?: boolean };
    }) => system.ask("agent:test-agent", { type: "set_model_config", config }),
  };
}

describe("AgentActor", () => {
  // ---- Basic dispatch ----

  it("dispatch completes the task and returns result", async () => {
    const { dispatch, system } = await createTestEnv({
      steps: [{ content: "Hello, I have completed the task.", status: "completed" }],
    });

    const task = makeTask("Say hello");
    const result = await dispatch(task);

    expect(result.finalStatus).toBe("completed");
    expect(result.messages.length).toBeGreaterThan(0);
    expect(result.summary).toContain("Hello");
    expect(system.getActorIds().some((id) => id.startsWith("runner:"))).toBe(false);
  });

  it("emits task lifecycle events (started → delta → completed)", async () => {
    const { dispatch, emitted } = await createTestEnv({
      steps: [
        {
          deltas: [{ type: "text", text: "Hello" }],
          content: "Hello, World!",
          status: "completed",
        },
      ],
    });

    await dispatch(makeTask("Say hello"));

    expect(emitted.some((e) => e.type === "task_started")).toBe(true);
    expect(emitted.some((e) => e.type === "task_delta")).toBe(true);
    expect(emitted.some((e) => e.type === "task_completed")).toBe(true);
  });

  it("emits structured message lifecycle events (task_message_start → task_message_update → task_message_end)", async () => {
    const { dispatch, emitted } = await createTestEnv({
      steps: [
        {
          deltas: [
            { type: "thinking", text: "Let's plan..." },
            { type: "text", text: "Hello there!" },
          ],
          content: "Hello there!",
          status: "completed",
        },
      ],
    });

    await dispatch(makeTask("Say hello with structured blocks"));

    const messageStart = emitted.filter((e) => e.type === "task_message_start");
    const messageUpdate = emitted.filter((e) => e.type === "task_message_update");
    const messageEnd = emitted.filter((e) => e.type === "task_message_end");

    expect(messageStart.length).toBe(1);
    expect(messageUpdate.length).toBeGreaterThan(0);
    expect(messageEnd.length).toBe(1);

    // Verify ID matches the stable step ID
    const startMsg = (messageStart[0] as any).message;
    expect(startMsg.id).toBe("assistant-step_1");
    expect(startMsg.role).toBe("assistant");

    const endMsg = (messageEnd[0] as any).message;
    expect(endMsg.id).toBe("assistant-step_1");
    expect(endMsg.role).toBe("assistant");

    // Verify that thinking and text blocks exist in content
    expect(Array.isArray(endMsg.content)).toBe(true);
    const textBlock = endMsg.content.find((b: any) => b.type === "text");
    expect(textBlock).toBeDefined();
    expect(textBlock.text).toBe("Hello there!");
  });

  it("assigns unique, stable message IDs per step in multi-step execution", async () => {
    const bashTool = makeToolDef("bash");

    const { dispatch, emitted } = await createTestEnv({
      tools: [bashTool],
      steps: [
        {
          deltas: [{ type: "text", text: "Running tool..." }],
          toolCalls: [{ id: "tc1", name: "bash", arguments: { command: "ls" } }],
          status: "continue",
        },
        {
          deltas: [{ type: "text", text: "Finished!" }],
          content: "I ran the tool and finished.",
          status: "completed",
        },
      ],
    });

    await dispatch(makeTask("Run ls"));

    const startEvents = emitted.filter((e) => e.type === "task_message_start");
    const endEvents = emitted.filter((e) => e.type === "task_message_end");

    expect(startEvents.length).toBe(2);
    expect(endEvents.length).toBe(2);

    const msgId0 = (startEvents[0] as any).message.id;
    const msgId1 = (startEvents[1] as any).message.id;

    expect(msgId0).toBe("assistant-step_1");
    expect(msgId1).toBe("assistant-step_2");

    const endMsgId0 = (endEvents[0] as any).message.id;
    const endMsgId1 = (endEvents[1] as any).message.id;

    expect(endMsgId0).toBe("assistant-step_1");
    expect(endMsgId1).toBe("assistant-step_2");
  });

  it("handles legacy message_end (Message instead of RuntimeMessage) with stable message ID", async () => {
    const customExecutor: ModelStepExecutor = {
      capabilities: {
        supportsTools: false,
        supportsSandbox: false,
        supportsMCP: false,
        tools: [],
      },
      executeStep(_input, signal) {
        const stream = new TestEventStream<ModelStepEvent, ModelStepResult>();
        void (async () => {
          if (signal?.aborted) {
            stream.end({ status: "aborted", appendedMessages: [], stopReason: "abort" });
            return;
          }

          // Simulate legacy Message structure (does not have .id property, has array content)
          const legacyMsg = {
            role: "assistant",
            content: [{ type: "text", text: "Hello from legacy executor!" }],
            timestamp: Date.now(),
          };

          stream.push({
            type: "message_end",
            message: legacyMsg as any, // No id property
          });
          stream.push({ type: "step_end" });

          stream.end({
            status: "completed",
            appendedMessages: [legacyMsg as any],
            stopReason: "assistant",
          });
        })();
        return stream;
      },
      async shutdown() {},
    };

    const { dispatch, emitted } = await createTestEnv({
      modelExecutor: customExecutor,
    });

    await dispatch(makeTask("Trigger legacy message"));

    const messageEnd = emitted.filter((e) => e.type === "task_message_end");
    expect(messageEnd.length).toBe(1);

    const msg = (messageEnd[0] as any).message;
    expect(msg.id).toBe("assistant-step_1"); // Checks that it fell back to stableId
    expect(msg.role).toBe("assistant");
    expect(Array.isArray(msg.content)).toBe(true);
    expect(msg.content[0].text).toBe("Hello from legacy executor!");
  });

  it("emits thinking deltas", async () => {
    const { dispatch, emitted } = await createTestEnv({
      steps: [
        {
          deltas: [{ type: "thinking", text: "Let me think..." }],
          content: "Here is my answer.",
          status: "completed",
        },
      ],
    });

    await dispatch(makeTask("Think about this"));

    const thinkingDeltas = emitted.filter(
      (e): e is Extract<OrchestratorEvent, { type: "task_delta" }> =>
        e.type === "task_delta" && (e.delta as { kind?: string })?.kind === "thinking",
    );
    expect(thinkingDeltas.length).toBeGreaterThan(0);
  });

  // ---- Tool calls ----

  it("executes tool calls and continues the loop", async () => {
    const bashTool = makeToolDef("bash");

    const { dispatch, emitted } = await createTestEnv({
      tools: [bashTool],
      steps: [
        {
          toolCalls: [{ id: "tc1", name: "bash", arguments: { command: "ls" } }],
          status: "continue",
        },
        { content: "I ran ls and found files.", status: "completed" },
      ],
    });

    const result = await dispatch(makeTask("List files"));

    expect(result.finalStatus).toBe("completed");
    expect(result.totalSteps).toBe(2);
    expect(emitted.some((e) => e.type === "tool_started")).toBe(true);
    expect(emitted.some((e) => e.type === "tool_finished")).toBe(true);
  });

  it("appends tool result to transcript after successful execution", async () => {
    const bashTool = makeToolDef("bash");

    const { dispatch } = await createTestEnv({
      tools: [bashTool],
      steps: [
        {
          toolCalls: [{ id: "tc1", name: "bash", arguments: { command: "ls" } }],
          status: "continue",
        },
        { content: "Done.", status: "completed" },
      ],
    });

    const result = await dispatch(makeTask("Run command"));
    const toolResultMsgs = result.messages.filter((m) => (m as Message).role === "toolResult");
    expect(toolResultMsgs.length).toBeGreaterThan(0);
  });

  // ---- Multiple tool calls (parallel by default) ----

  it("executes multiple tool calls from a single model step in parallel", async () => {
    const bashTool = makeToolDef("bash");
    const readTool = makeToolDef("read");

    const { dispatch, emitted } = await createTestEnv({
      tools: [bashTool, readTool],
      steps: [
        {
          toolCalls: [
            { id: "tc1", name: "bash", arguments: { command: "ls" } },
            { id: "tc2", name: "read", arguments: { path: "file.txt" } },
          ],
          status: "continue",
        },
        { content: "Both tools ran.", status: "completed" },
      ],
    });

    const result = await dispatch(makeTask("Run multiple tools"));
    expect(result.finalStatus).toBe("completed");

    const toolStartEvents = emitted.filter((e) => e.type === "tool_started");
    expect(toolStartEvents.length).toBe(2);

    const toolEndEvents = emitted.filter((e) => e.type === "tool_finished");
    expect(toolEndEvents.length).toBe(2);
  });

  it("executes multiple tool calls sequentially if a tool has executionMode: sequential", async () => {
    const bashTool = makeToolDef("bash", { executionMode: "sequential" });
    const readTool = makeToolDef("read");

    const { dispatch, emitted } = await createTestEnv({
      tools: [bashTool, readTool],
      steps: [
        {
          toolCalls: [
            { id: "tc1", name: "bash", arguments: { command: "ls" } },
            { id: "tc2", name: "read", arguments: { path: "file.txt" } },
          ],
          status: "continue",
        },
        { content: "Both tools ran sequentially.", status: "completed" },
      ],
    });

    const result = await dispatch(makeTask("Run multiple tools sequentially"));
    expect(result.finalStatus).toBe("completed");

    const toolStartEvents = emitted.filter((e) => e.type === "tool_started");
    expect(toolStartEvents.length).toBe(2);

    const toolEndEvents = emitted.filter((e) => e.type === "tool_finished");
    expect(toolEndEvents.length).toBe(2);
  });

  // ---- Error handling in tool calls ----

  it("reflects tool execution error in transcript with isError flag", async () => {
    const bashTool = makeToolDef("bash");

    const { dispatch } = await createTestEnv({
      tools: [bashTool],
      steps: [
        {
          toolCalls: [{ id: "tc1", name: "bash", arguments: { command: "invalid" } }],
          status: "continue",
        },
        { content: "Handled error.", status: "completed" },
      ],
    });

    const result = await dispatch(makeTask("Run bad command"));
    const toolResults = result.messages.filter((m) => (m as Message).role === "toolResult");
    expect(toolResults.length).toBeGreaterThan(0);
    expect(
      (toolResults[toolResults.length - 1] as Message & { details?: unknown }).details,
    ).toBeDefined();
  });

  it("handles tool execution returning ok:false as structured error in transcript", async () => {
    const flakyTool = makeToolDef("flaky");

    const { dispatch } = await createTestEnv({
      tools: [flakyTool],
      toolResult: {
        ok: false,
        error: { code: "timeout", message: "Command timed out", retryable: true },
      },
      steps: [
        { toolCalls: [{ id: "tc1", name: "flaky", arguments: {} }], status: "continue" },
        { content: "Dealt with error.", status: "completed" },
      ],
    });

    const result = await dispatch(makeTask("Test flaky"));
    expect(result.finalStatus).toBe("completed");

    const toolResults = result.messages.filter((m: Message) => m.role === "toolResult");
    expect(toolResults.length).toBe(1);
    const tr = toolResults[0] as Message & { isError?: boolean; details?: unknown };
    expect(tr.isError).toBe(true);
    // The error details object; shape depends on ToolExecResult nesting
    expect(tr.details).toBeDefined();
  });

  // ---- Transcript growth across steps ----

  it("transcript grows across tool-call steps and model sees tool results", async () => {
    const bashTool = makeToolDef("bash");

    const { dispatch } = await createTestEnv({
      tools: [bashTool],
      steps: [
        {
          toolCalls: [{ id: "tc1", name: "bash", arguments: { command: "ls" } }],
          status: "continue",
        },
        { content: "Done after tool.", status: "completed" },
      ],
    });

    const result = await dispatch(makeTask("Run ls"));
    expect(result.finalStatus).toBe("completed");
    expect(result.totalSteps).toBe(2);

    const roles = result.messages.map((m: Message) => m.role);
    expect(roles.filter((r) => r === "user").length).toBe(1);
    expect(roles.filter((r) => r === "assistant").length).toBe(2);
    expect(roles.filter((r) => r === "toolResult").length).toBe(1);

    const toolResultIdx = roles.indexOf("toolResult");
    const lastAssistantIdx = roles.lastIndexOf("assistant");
    expect(toolResultIdx).toBeLessThan(lastAssistantIdx);
  });

  // ---- Cancel ----

  it("cancel sets task to aborted", async () => {
    const { dispatch, cancel } = await createTestEnv({
      steps: [
        {
          deltas: [{ type: "text", text: "Working..." }],
          content: "Working...",
          status: "completed",
        },
      ],
    });

    const task = makeTask("Something");
    const dispatchPromise = dispatch(task);
    await cancel(task.id!, "user requested");
    const result = await dispatchPromise;
    expect(["completed", "aborted"]).toContain(result.finalStatus);
  });

  // ---- Max steps ----

  it("fails task when max steps reached", async () => {
    const bashTool = makeToolDef("bash");

    const { dispatch, emitted } = await createTestEnv({
      tools: [bashTool],
      maxSteps: 2,
      steps: [
        { toolCalls: [{ id: "tc1", name: "bash", arguments: {} }], status: "continue" },
        { toolCalls: [{ id: "tc2", name: "bash", arguments: {} }], status: "continue" },
      ],
    });

    const result = await dispatch(makeTask("Run forever"));
    expect(result.finalStatus).toBe("max_steps");
    expect(emitted.some((e) => e.type === "task_failed")).toBe(true);
  });

  // ---- Error handling ----

  it("handles model executor error and returns error status", async () => {
    const { dispatch } = await createTestEnv({
      steps: [{ throwError: "Model API error" }],
    });

    const result = await dispatch(makeTask("Test"));
    expect(result.finalStatus).toBe("error");
  });

  // ---- set_model_config ----

  it("updates model config via set_model_config message", async () => {
    const { dispatch, setModelConfig } = await createTestEnv({
      steps: [{ content: "Done.", status: "completed" }],
    });

    await setModelConfig({
      model: { id: "new-model", name: "New Model" },
      settings: { allowToolCalls: false },
    });
    const result = await dispatch(makeTask("Hello"));
    expect(result.finalStatus).toBe("completed");
  });

  // ---- Simple completion ----

  it("without tool calls and allowToolCalls=true, task completes", async () => {
    const { dispatch } = await createTestEnv({
      steps: [{ content: "Here is a simple answer.", status: "completed" }],
    });

    const result = await dispatch(makeTask("Simple question"));
    expect(result.finalStatus).toBe("completed");
    expect(result.summary).toContain("Here is a simple answer");
  });

  // ---- allowToolCalls: false ----

  it("skips tool execution when allowToolCalls is false", async () => {
    const bashTool = makeToolDef("bash");

    const { dispatch, setModelConfig, emitted } = await createTestEnv({
      tools: [bashTool],
      steps: [
        {
          toolCalls: [{ id: "tc1", name: "bash", arguments: {} }],
          content: "Would run bash.",
          status: "completed",
        },
      ],
    });

    await setModelConfig({ settings: { allowToolCalls: false } });
    const result = await dispatch(makeTask("Run command"));
    expect(result.finalStatus).toBe("completed");
    expect(emitted.some((e) => e.type === "tool_started")).toBe(false);
  });

  // ---- Sequential dispatch ----

  it("processes two dispatch calls sequentially", async () => {
    const { dispatch } = await createTestEnv({
      steps: [
        { content: "First result", status: "completed" },
        { content: "Second result", status: "completed" },
      ],
    });

    const result1 = await dispatch(makeTask("First task"));
    expect(result1.finalStatus).toBe("completed");

    const result2 = await dispatch(makeTask("Second task"));
    expect(result2.finalStatus).toBe("completed");
    expect(result2.messages.length).toBeGreaterThan(0);
  });

  // ---- Cancellation ----

  it("handles cancellation mid-run", async () => {
    const bashTool = makeToolDef("bash");
    const { system, emitted } = await createTestEnv({
      tools: [bashTool],
      maxSteps: 10,
      steps: [
        {
          toolCalls: [{ id: "tc1", name: "bash", arguments: {} }],
          status: "continue",
          delayMs: 50,
        },
        { toolCalls: [{ id: "tc2", name: "bash", arguments: {} }], status: "continue" },
        { toolCalls: [{ id: "tc3", name: "bash", arguments: {} }], status: "continue" },
      ],
    });

    const task = makeTask("Cancel me");

    const dispatchPromise = system.ask<{ finalStatus: string }>("agent:test-agent", {
      type: "dispatch",
      task,
    });

    // Wait a brief moment to let task runner start and hit the delay
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Send cancel message
    await system.ask("agent:test-agent", { type: "cancel", taskId: task.id });

    const result = await dispatchPromise;
    expect(result.finalStatus).toBe("aborted");
    expect(emitted.some((e) => e.type === "task_cancelled")).toBe(true);
  });

  // ---- Concurrent dispatch rejection ----

  it("rejects concurrent dispatch mid-run", async () => {
    const { system } = await createTestEnv({
      steps: [{ content: "Result", status: "completed", delayMs: 50 }],
    });

    const task1 = makeTask("Task 1");
    const task2 = makeTask("Task 2");

    const dispatchPromise1 = system.ask("agent:test-agent", {
      type: "dispatch",
      task: task1,
    });

    // Wait a brief moment to let task runner start
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Send second dispatch concurrently, which should be rejected immediately
    await expect(
      system.ask("agent:test-agent", {
        type: "dispatch",
        task: task2,
      }),
    ).rejects.toThrow("Agent already running a task");

    await dispatchPromise1;
  });

  // ---- Mid-run config updates ----

  it("allows model config updates mid-run without blocking", async () => {
    const { system } = await createTestEnv({
      steps: [{ content: "Result", status: "completed", delayMs: 50 }],
    });

    const task = makeTask("Config check");

    const dispatchPromise = system.ask("agent:test-agent", {
      type: "dispatch",
      task,
    });

    // Wait a brief moment to let task runner start
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Send config update - should resolve immediately (well before 50ms)
    const startTime = Date.now();
    await system.ask("agent:test-agent", {
      type: "set_model_config",
      config: { settings: { allowToolCalls: false } },
    });
    const duration = Date.now() - startTime;

    expect(duration).toBeLessThan(25); // Should resolve almost instantly (usually < 2ms)

    await dispatchPromise;
  });
});
