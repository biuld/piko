import type { FauxProviderRegistration } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, fauxToolCall, registerFauxProvider } from "@earendil-works/pi-ai";
import type {
  EngineEvent,
  EngineInput,
  EngineModel,
  EngineStepResult,
  EngineTool,
} from "piko-engine-protocol";
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

function buildTestModel(): EngineModel {
  return {
    id: MODEL_ID,
    name: "Test Model",
    api: API,
    provider: PROVIDER,
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
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
