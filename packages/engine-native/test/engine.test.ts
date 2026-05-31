import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, fauxToolCall, registerFauxProvider } from "@earendil-works/pi-ai";
import type { EngineEvent, EngineInput, EngineStepResult, EngineTool } from "piko-engine-protocol";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createNativeEngine } from "../src/engine.js";

const PROVIDER = "faux";
const API = "openai-completions";
const MODEL_ID = "faux-engine-model";

let faux: FauxProviderRegistration;

beforeAll(() => {
  faux = registerFauxProvider({
    api: API,
    provider: PROVIDER,
    models: [{ id: MODEL_ID }],
  });
});

afterAll(() => {
  faux?.unregister();
});

// ---- Test helpers ----

function buildTestModel(): Model<string> {
  return {
    id: MODEL_ID,
    name: "Test Model",
    api: API,
    provider: PROVIDER,
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

function buildBaseInput(overrides?: Partial<EngineInput>): EngineInput {
  return {
    runId: "test-run",
    stepId: "test-step",
    transcript: [{ role: "user", content: "Hello", timestamp: Date.now() }],
    systemPrompt: "You are a helpful assistant.",
    model: buildTestModel(),
    provider: {},
    tools: [],
    settings: {
      maxSteps: 10,
      parallelTools: false,
      allowToolCalls: true,
      allowApprovals: false,
    },
    ...overrides,
  };
}

async function collectStepResult(
  engine: ReturnType<typeof createNativeEngine>,
  input: EngineInput,
): Promise<{ events: EngineEvent[]; result: EngineStepResult }> {
  const stream = engine.executeStep(input);
  const events: EngineEvent[] = [];
  for await (const event of stream) events.push(event);
  return { events, result: await stream.result() };
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function createTestTool(name: string, executionMode?: EngineTool["executionMode"]): EngineTool {
  return {
    name,
    description: `Tool ${name}`,
    inputSchema: { type: "object", properties: { value: { type: "string" } } },
    executor: { kind: "native", target: name },
    executionMode,
  };
}

// ---- Tests ----

describe("NativeEngine", () => {
  describe("assistant-only step", () => {
    it("should return completed with assistant message", async () => {
      faux.setResponses([fauxAssistantMessage("Hello from test!")]);

      const engine = createNativeEngine({ toolRegistry: {} });
      const input = buildBaseInput({
        tools: [],
        settings: {
          maxSteps: 10,
          parallelTools: false,
          allowToolCalls: false,
          allowApprovals: false,
          stopConditions: { stopOnAssistantMessage: true },
        },
      });

      const { result } = await collectStepResult(engine, input);

      expect(result.status).toBe("completed");
      expect(result.appendedMessages).toHaveLength(1);
      const msg = result.appendedMessages[0];
      if (msg.role === "assistant") {
        expect(msg.content[0]).toMatchObject({ type: "text", text: "Hello from test!" });
      }
    });
  });

  describe("assistant + tool step", () => {
    it("should execute tool and return results", async () => {
      faux.setResponses([
        fauxAssistantMessage([fauxToolCall("test_tool", { query: "test" }, { id: "call_1" })]),
      ]);

      const testTool: EngineTool = {
        name: "test_tool",
        description: "A test tool",
        inputSchema: { type: "object", properties: { query: { type: "string" } } },
        executor: { kind: "native", target: "test_tool" },
      };

      const engine = createNativeEngine({
        toolRegistry: {
          test_tool: async (args) => ({ result: `executed with ${args.query}` }),
        },
        toolDefinitions: [testTool],
      });

      const input = buildBaseInput({ tools: [testTool] });
      const { events, result } = await collectStepResult(engine, input);

      expect(result.status).toBe("continue");
      expect(result.appendedMessages).toHaveLength(2);
      expect(result.appendedMessages[0].role).toBe("assistant");
      expect(result.appendedMessages[1].role).toBe("toolResult");
      if (result.appendedMessages[1]?.role === "toolResult") {
        expect(result.appendedMessages[1].details).toEqual({ result: "executed with test" });
      }

      const toolEndEvent = events.find((e) => e.type === "tool_call_end");
      expect(toolEndEvent).toBeDefined();
      if (toolEndEvent?.type === "tool_call_end") expect(toolEndEvent.isError).toBe(false);
    });

    it("executes tool calls in parallel by default", async () => {
      faux.setResponses([
        fauxAssistantMessage([
          fauxToolCall("parallel_one", { value: "one" }, { id: "call_parallel_1" }),
          fauxToolCall("parallel_two", { value: "two" }, { id: "call_parallel_2" }),
        ]),
      ]);

      const tools = [createTestTool("parallel_one"), createTestTool("parallel_two")];
      let active = 0;
      let maxActive = 0;
      const runTool = async (args: Record<string, unknown>) => {
        active += 1;
        maxActive = Math.max(maxActive, active);
        await delay(20);
        active -= 1;
        return { value: args.value };
      };
      const engine = createNativeEngine({
        toolRegistry: {
          parallel_one: runTool,
          parallel_two: runTool,
        },
        toolDefinitions: tools,
      });

      const input = buildBaseInput({
        tools,
        settings: {
          maxSteps: 10,
          allowToolCalls: true,
          allowApprovals: false,
        },
      });
      const { result } = await collectStepResult(engine, input);

      expect(result.status).toBe("continue");
      expect(maxActive).toBe(2);
    });

    it("preserves tool result message order when parallel calls finish out of order", async () => {
      faux.setResponses([
        fauxAssistantMessage([
          fauxToolCall("slow_tool", { value: "slow" }, { id: "call_order_1" }),
          fauxToolCall("fast_tool", { value: "fast" }, { id: "call_order_2" }),
        ]),
      ]);

      const tools = [createTestTool("slow_tool"), createTestTool("fast_tool")];
      const engine = createNativeEngine({
        toolRegistry: {
          slow_tool: async () => {
            await delay(30);
            return { value: "slow" };
          },
          fast_tool: async () => {
            await delay(1);
            return { value: "fast" };
          },
        },
        toolDefinitions: tools,
      });

      const input = buildBaseInput({
        tools,
        settings: {
          maxSteps: 10,
          allowToolCalls: true,
          allowApprovals: false,
        },
      });
      const { events, result } = await collectStepResult(engine, input);

      const toolMessages = result.appendedMessages.filter((m) => m.role === "toolResult");
      expect(toolMessages.map((m) => (m.role === "toolResult" ? m.toolCallId : ""))).toEqual([
        "call_order_1",
        "call_order_2",
      ]);
      expect(toolMessages.map((m) => (m.role === "toolResult" ? m.details : undefined))).toEqual([
        { value: "slow" },
        { value: "fast" },
      ]);

      const toolEndIds = events
        .filter((event) => event.type === "tool_call_end")
        .map((event) => (event.type === "tool_call_end" ? event.id : ""));
      expect(toolEndIds).toEqual(["call_order_2", "call_order_1"]);
    });

    it("executes the whole batch sequentially when settings disable parallel tools", async () => {
      faux.setResponses([
        fauxAssistantMessage([
          fauxToolCall("settings_seq_one", { value: "one" }, { id: "call_settings_1" }),
          fauxToolCall("settings_seq_two", { value: "two" }, { id: "call_settings_2" }),
        ]),
      ]);

      const tools = [createTestTool("settings_seq_one"), createTestTool("settings_seq_two")];
      const executionOrder: string[] = [];
      const runTool = async (args: Record<string, unknown>) => {
        executionOrder.push(String(args.value));
        await delay(5);
        return { value: args.value };
      };
      const engine = createNativeEngine({
        toolRegistry: {
          settings_seq_one: runTool,
          settings_seq_two: runTool,
        },
        toolDefinitions: tools,
      });

      const input = buildBaseInput({
        tools,
        settings: {
          maxSteps: 10,
          parallelTools: false,
          allowToolCalls: true,
          allowApprovals: false,
        },
      });
      const { result } = await collectStepResult(engine, input);

      expect(result.status).toBe("continue");
      expect(executionOrder).toEqual(["one", "two"]);
    });

    it("executes the whole batch sequentially when any called tool is sequential", async () => {
      faux.setResponses([
        fauxAssistantMessage([
          fauxToolCall("metadata_parallel", { value: "parallel" }, { id: "call_metadata_1" }),
          fauxToolCall("metadata_sequential", { value: "sequential" }, { id: "call_metadata_2" }),
        ]),
      ]);

      const tools = [
        createTestTool("metadata_parallel", "parallel"),
        createTestTool("metadata_sequential", "sequential"),
      ];
      let active = 0;
      let maxActive = 0;
      const executionOrder: string[] = [];
      const runTool = async (args: Record<string, unknown>) => {
        active += 1;
        maxActive = Math.max(maxActive, active);
        executionOrder.push(String(args.value));
        await delay(5);
        active -= 1;
        return { value: args.value };
      };
      const engine = createNativeEngine({
        toolRegistry: {
          metadata_parallel: runTool,
          metadata_sequential: runTool,
        },
        toolDefinitions: tools,
      });

      const input = buildBaseInput({
        tools,
        settings: {
          maxSteps: 10,
          allowToolCalls: true,
          allowApprovals: false,
        },
      });
      const { result } = await collectStepResult(engine, input);

      expect(result.status).toBe("continue");
      expect(maxActive).toBe(1);
      expect(executionOrder).toEqual(["parallel", "sequential"]);
    });
  });

  describe("tool error", () => {
    it("should report tool errors as toolResult with isError", async () => {
      faux.setResponses([
        fauxAssistantMessage([fauxToolCall("failing_tool", { input: "bad" }, { id: "call_2" })]),
      ]);

      const testTool: EngineTool = {
        name: "failing_tool",
        description: "Always fails",
        inputSchema: { type: "object", properties: { input: { type: "string" } } },
        executor: { kind: "native", target: "failing_tool" },
      };

      const engine = createNativeEngine({
        toolRegistry: {
          failing_tool: async () => {
            throw new Error("Tool execution failed");
          },
        },
        toolDefinitions: [testTool],
      });

      const input = buildBaseInput({ tools: [testTool] });
      const { events, result } = await collectStepResult(engine, input);

      expect(result.status).toBe("continue");
      const toolMsg = result.appendedMessages.find((m) => m.role === "toolResult");
      expect(toolMsg).toBeDefined();
      if (toolMsg?.role === "toolResult") expect(toolMsg.isError).toBe(true);

      const toolEndEvent = events.find((e) => e.type === "tool_call_end");
      expect(toolEndEvent).toBeDefined();
      if (toolEndEvent?.type === "tool_call_end") expect(toolEndEvent.isError).toBe(true);
    });
  });

  describe("approval pause", () => {
    it("should return awaiting_approval when tool needs approval", async () => {
      faux.setResponses([
        fauxAssistantMessage([
          fauxToolCall("approval_tool", { action: "delete" }, { id: "call_3" }),
        ]),
      ]);

      const approvalTool: EngineTool = {
        name: "approval_tool",
        description: "Needs approval",
        inputSchema: { type: "object", properties: { action: { type: "string" } } },
        executor: { kind: "native", target: "approval_tool" },
        metadata: { requiresApproval: true },
      };

      const engine = createNativeEngine({
        toolRegistry: { approval_tool: async () => ({ ok: true }) },
        toolDefinitions: [approvalTool],
      });

      const input = buildBaseInput({
        tools: [approvalTool],
        settings: {
          maxSteps: 10,
          parallelTools: false,
          allowToolCalls: true,
          allowApprovals: true,
        },
      });

      const { events, result } = await collectStepResult(engine, input);

      expect(result.status).toBe("awaiting_approval");
      expect(result.pendingApproval?.kind).toBe("tool:approval_tool");
      expect(result.stopReason).toBe("approval");

      const approvalEvent = events.find((e) => e.type === "approval_requested");
      expect(approvalEvent).toBeDefined();
    });
  });

  describe("stop conditions", () => {
    it("should stop on assistant message when configured", async () => {
      faux.setResponses([fauxAssistantMessage("Single response")]);

      const engine = createNativeEngine({ toolRegistry: {} });
      const input = buildBaseInput({
        tools: [],
        settings: {
          maxSteps: 10,
          parallelTools: false,
          allowToolCalls: true,
          allowApprovals: false,
          stopConditions: { stopOnAssistantMessage: true },
        },
      });

      const { result } = await collectStepResult(engine, input);

      expect(result.status).toBe("completed");
      expect(result.stopReason).toBe("assistant");
    });
  });
});
