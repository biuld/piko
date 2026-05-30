import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { registerFauxProvider, fauxText, fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import type { FauxProviderRegistration } from "@earendil-works/pi-ai";
import { createNativeEngine } from "../src/engine.ts";
import type {
  EngineInput,
  EngineTool,
  EngineEvent,
  EngineStepResult,
} from "piko-engine-protocol";
import type { Message } from "@earendil-works/pi-ai";

const PROVIDER = "faux";
const API = "openai-completions";
const MODEL_ID = "faux-test-model";

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

function buildBaseInput(overrides?: Partial<EngineInput>): EngineInput {
  const model = faux.getModel();
  return {
    runId: "test-run",
    stepId: "test-step",
    transcript: [
      {
        role: "user",
        content: "Hello",
        timestamp: Date.now(),
      },
    ],
    systemPrompt: "You are a helpful assistant.",
    model,
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
  for await (const event of stream) {
    events.push(event);
  }
  const result = await stream.result();
  return { events, result };
}

describe("NativeEngine", () => {
  describe("assistant-only step", () => {
    it("should return completed with assistant message", async () => {
      faux.setResponses([fauxAssistantMessage("Hello from faux!")]);

      const engine = createNativeEngine();
      const input = buildBaseInput({
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
        expect(msg.content[0]).toMatchObject({ type: "text", text: "Hello from faux!" });
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
        inputSchema: {
          type: "object",
          properties: {
            query: { type: "string" },
          },
        },
        executor: {
          kind: "native",
          target: "test_tool",
        },
      };

      const engine = createNativeEngine({
        tools: {
          test_tool: async (args) => {
            return { result: `executed with ${args.query}` };
          },
        },
      });

      const input = buildBaseInput({
        tools: [testTool],
      });

      const { events, result } = await collectStepResult(engine, input);

      expect(result.status).toBe("continue");
      // Should have assistant message + tool result
      expect(result.appendedMessages).toHaveLength(2);
      expect(result.appendedMessages[0].role).toBe("assistant");
      expect(result.appendedMessages[1].role).toBe("toolResult");

      // Check tool_call_end event
      const toolEndEvent = events.find((e) => e.type === "tool_call_end");
      expect(toolEndEvent).toBeDefined();
      if (toolEndEvent?.type === "tool_call_end") {
        expect(toolEndEvent.isError).toBe(false);
      }
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
        inputSchema: {
          type: "object",
          properties: { input: { type: "string" } },
        },
        executor: {
          kind: "native",
          target: "failing_tool",
        },
      };

      const engine = createNativeEngine({
        tools: {
          failing_tool: async () => {
            throw new Error("Tool execution failed");
          },
        },
      });

      const input = buildBaseInput({ tools: [testTool] });

      const { events, result } = await collectStepResult(engine, input);

      expect(result.status).toBe("continue");
      const toolMsg = result.appendedMessages.find((m) => m.role === "toolResult");
      expect(toolMsg).toBeDefined();
      if (toolMsg && toolMsg.role === "toolResult") {
        expect(toolMsg.isError).toBe(true);
      }

      const toolEndEvent = events.find((e) => e.type === "tool_call_end");
      expect(toolEndEvent).toBeDefined();
      if (toolEndEvent?.type === "tool_call_end") {
        expect(toolEndEvent.isError).toBe(true);
      }
    });
  });

  describe("approval pause", () => {
    it("should return awaiting_approval when tool needs approval", async () => {
      faux.setResponses([
        fauxAssistantMessage([fauxToolCall("approval_tool", { action: "delete" }, { id: "call_3" })]),
      ]);

      const approvalTool: EngineTool = {
        name: "approval_tool",
        description: "Needs approval",
        inputSchema: {
          type: "object",
          properties: { action: { type: "string" } },
        },
        executor: {
          kind: "native",
          target: "approval_tool",
        },
        metadata: { requiresApproval: true },
      };

      const engine = createNativeEngine({
        tools: {
          approval_tool: async () => ({ ok: true }),
        },
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
      expect(result.pendingApproval).toBeDefined();
      expect(result.pendingApproval?.kind).toBe("tool:approval_tool");
      expect(result.stopReason).toBe("approval");

      const approvalEvent = events.find((e) => e.type === "approval_requested");
      expect(approvalEvent).toBeDefined();
    });
  });

  describe("stop conditions", () => {
    it("should stop on assistant message when configured", async () => {
      faux.setResponses([fauxAssistantMessage("Single response")]);

      const engine = createNativeEngine();
      const input = buildBaseInput({
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
