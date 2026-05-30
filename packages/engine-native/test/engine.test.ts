import { describe, it, expect } from "vitest";
import { EventStream } from "piko-engine-protocol";
import type {
  EngineInput,
  EngineTool,
  EngineEvent,
  EngineStepResult,
  EngineModel,
  Message,
} from "piko-engine-protocol";
import type { LlmCaller, LlmCallInput, LlmEvent, LlmResult } from "../src/llm-caller.js";
import { createNativeEngine } from "../src/engine.js";

// ---- Fake LlmCaller ----

function createFakeLlmCaller(
  responses: Message[],
): LlmCaller {
  let callCount = 0;

  return {
    call(input: LlmCallInput, signal?: AbortSignal): EventStream<LlmEvent, LlmResult> {
      const stream = new EventStream<LlmEvent, LlmResult>();
      const response = responses[callCount++] ?? responses[responses.length - 1];
      const msg = { ...response, timestamp: Date.now() };

      // Simulate streaming: push text deltas or tool call starts
      if (msg.role === "assistant") {
        const content = Array.isArray(msg.content) ? msg.content : [];
        for (const block of content) {
          if (block.type === "text") {
            stream.push({ type: "text_delta", delta: block.text });
          } else if (block.type === "toolCall") {
            stream.push({ type: "tool_call_start", id: block.id, name: block.name });
          }
        }
      }

      // Resolve asynchronously
      queueMicrotask(() => {
        stream.end({ message: msg, usage: { input: 10, output: 5, total: 15 } });
      });

      return stream;
    },
  };
}

// ---- Test helpers ----

function buildTestModel(): EngineModel {
  return {
    id: "test-model",
    name: "Test Model",
    api: "openai-completions",
    provider: "test-provider",
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
    transcript: [
      {
        role: "user",
        content: "Hello",
        timestamp: Date.now(),
      },
    ],
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
  for await (const event of stream) {
    events.push(event);
  }
  const result = await stream.result();
  return { events, result };
}

function makeAssistant(text: string): Message {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    timestamp: Date.now(),
  };
}

function makeToolCall(name: string, args: Record<string, unknown>, id: string): Message {
  return {
    role: "assistant",
    content: [{ type: "toolCall", id, name, arguments: args }],
    timestamp: Date.now(),
  };
}

// ---- Tests ----

describe("NativeEngine", () => {
  describe("assistant-only step", () => {
    it("should return completed with assistant message", async () => {
      const llmCaller = createFakeLlmCaller([makeAssistant("Hello from test!")]);
      const engine = createNativeEngine({ llmCaller });
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
        expect(msg.content[0]).toMatchObject({ type: "text", text: "Hello from test!" });
      }
    });
  });

  describe("assistant + tool step", () => {
    it("should execute tool and return results", async () => {
      const llmCaller = createFakeLlmCaller([
        makeToolCall("test_tool", { query: "test" }, "call_1"),
      ]);

      const testTool: EngineTool = {
        name: "test_tool",
        description: "A test tool",
        inputSchema: {
          type: "object",
          properties: { query: { type: "string" } },
        },
        executor: { kind: "native", target: "test_tool" },
      };

      const engine = createNativeEngine({
        llmCaller,
        tools: {
          test_tool: async (args) => {
            return { result: `executed with ${args.query}` };
          },
        },
      });

      const input = buildBaseInput({ tools: [testTool] });

      const { events, result } = await collectStepResult(engine, input);

      expect(result.status).toBe("continue");
      expect(result.appendedMessages).toHaveLength(2);
      expect(result.appendedMessages[0].role).toBe("assistant");
      expect(result.appendedMessages[1].role).toBe("toolResult");

      const toolEndEvent = events.find((e) => e.type === "tool_call_end");
      expect(toolEndEvent).toBeDefined();
      if (toolEndEvent?.type === "tool_call_end") {
        expect(toolEndEvent.isError).toBe(false);
      }
    });
  });

  describe("tool error", () => {
    it("should report tool errors as toolResult with isError", async () => {
      const llmCaller = createFakeLlmCaller([
        makeToolCall("failing_tool", { input: "bad" }, "call_2"),
      ]);

      const testTool: EngineTool = {
        name: "failing_tool",
        description: "Always fails",
        inputSchema: {
          type: "object",
          properties: { input: { type: "string" } },
        },
        executor: { kind: "native", target: "failing_tool" },
      };

      const engine = createNativeEngine({
        llmCaller,
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
      const llmCaller = createFakeLlmCaller([
        makeToolCall("approval_tool", { action: "delete" }, "call_3"),
      ]);

      const approvalTool: EngineTool = {
        name: "approval_tool",
        description: "Needs approval",
        inputSchema: {
          type: "object",
          properties: { action: { type: "string" } },
        },
        executor: { kind: "native", target: "approval_tool" },
        metadata: { requiresApproval: true },
      };

      const engine = createNativeEngine({
        llmCaller,
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
      const llmCaller = createFakeLlmCaller([makeAssistant("Single response")]);
      const engine = createNativeEngine({ llmCaller });
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
