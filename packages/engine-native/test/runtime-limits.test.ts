import { describe, expect, it } from "bun:test";
import type { EngineInput } from "piko-engine-protocol";
import { createNativeEngine } from "../src/engine.js";
import {
  checkBeforeApproval,
  checkBeforeModelCall,
  checkBeforeToolCall,
  createCounters,
  withToolTimeout,
} from "../src/runtime-limits.js";
import { buildAssistantMessage } from "../src/transcript-builder.js";
import type { NativeToolRegistry } from "../src/types.js";
import { emptyUsage, makeFauxAdapter, makeModel, makeSettings } from "./helpers.js";

describe("Runtime Limits", () => {
  it("should allow calls within limits", () => {
    const counters = createCounters();
    counters.modelCalls = 3;
    const result = checkBeforeModelCall(counters, { maxModelCalls: 10 });
    expect(result).toBeNull();
  });

  it("should detect max model calls exceeded", () => {
    const counters = createCounters();
    counters.modelCalls = 5;
    const result = checkBeforeModelCall(counters, { maxModelCalls: 5 });
    expect(result).toEqual({ exceeded: true, stopReason: "max_steps" });
  });

  it("should detect max tool calls exceeded", () => {
    const counters = createCounters();
    counters.toolCalls = 10;
    const result = checkBeforeToolCall(counters, { maxToolCalls: 10 });
    expect(result).toEqual({ exceeded: true, stopReason: "max_steps" });
  });

  it("should detect max wall clock exceeded", () => {
    const counters = createCounters();
    counters.startedAt = Date.now() - 10000;
    const result = checkBeforeModelCall(counters, { maxWallClockMs: 5000 });
    expect(result).toEqual({ exceeded: true, stopReason: "abort" });
  });

  it("should detect max consecutive errors", () => {
    const counters = createCounters();
    counters.consecutiveErrors = 5;
    const result = checkBeforeModelCall(counters, { maxConsecutiveErrors: 5 });
    expect(result).toEqual({ exceeded: true, stopReason: "error" });
  });

  it("should detect max approval requests exceeded", () => {
    const counters = createCounters();
    counters.approvalRequests = 3;
    const result = checkBeforeApproval(counters, { maxApprovalRequests: 3 });
    expect(result).toEqual({ exceeded: true, stopReason: "max_steps" });
  });

  it("should enforce per-tool timeout", async () => {
    let completed = false;
    try {
      await withToolTimeout(async () => {
        await new Promise((r) => setTimeout(r, 200));
        completed = true;
        return "done";
      }, 50);
      expect(false).toBe(true);
    } catch (err: unknown) {
      expect((err as Error).message).toContain("timed out");
      expect(completed).toBe(false);
    }
  });

  it("should respect AbortSignal", async () => {
    const controller = new AbortController();
    controller.abort();
    try {
      await withToolTimeout(async () => "done", undefined, controller.signal);
      expect(false).toBe(true);
    } catch (err: unknown) {
      expect((err as Error).message).toContain("Aborted");
    }
  });

  it("should return result when within timeout", async () => {
    const result = await withToolTimeout(async () => "success", 1000);
    expect(result).toBe("success");
  });
});

describe("Engine Step with Runtime Limits", () => {
  it("should stop when max model calls is reached", async () => {
    const engine = createNativeEngine({
      providerAdapter: makeFauxAdapter(() => ({
        messages: [buildAssistantMessage("hello", [])],
        usage: emptyUsage,
      })),
    });
    const input: EngineInput = {
      runId: "test-run",
      stepId: "test-step",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({
        allowToolCalls: true,
        runtimeLimits: { maxModelCalls: 0 },
      }),
    };
    const result = await engine.executeStep(input).result();
    expect(result.status).toBe("completed");
    expect(result.stopReason).toBe("max_steps");
  });

  it("should handle AbortSignal", async () => {
    const controller = new AbortController();
    controller.abort();
    const engine = createNativeEngine();
    const input: EngineInput = {
      runId: "test-run",
      stepId: "test-step",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({ allowToolCalls: true }),
    };
    const result = await engine.executeStep(input, controller.signal).result();
    expect(result.status).toBe("aborted");
  });

  it("should enforce maxToolCalls limit before batch (fix #2)", async () => {
    const registry: NativeToolRegistry = { echo: async () => "ok" };
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "echo",
          description: "Echo",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "echo" },
        },
      ],
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("echoing", [
            { type: "toolCall", id: "tc-1", name: "echo", arguments: {} },
          ]),
        ],
        usage: emptyUsage,
      })),
    });
    const input: EngineInput = {
      runId: "test-run",
      stepId: "step-1",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({ allowToolCalls: true, runtimeLimits: { maxToolCalls: 0 } }),
      engineState: {
        version: 1,
        counters: {
          modelCalls: 0,
          toolCalls: 0,
          approvalRequests: 0,
          consecutiveErrors: 0,
          startedAt: Date.now(),
        },
      },
    };
    const result = await engine.executeStep(input).result();
    expect(result.stopReason).toBe("max_steps");
    expect(result.appendedMessages.length).toBe(1);
    expect(result.appendedMessages[0].role).toBe("assistant");
  });

  it("should limit tool calls within a single batch (per-call enforcement)", async () => {
    const executed: string[] = [];
    const registry: NativeToolRegistry = {
      a: async () => {
        executed.push("a");
        return "ok";
      },
      b: async () => {
        executed.push("b");
        return "ok";
      },
      c: async () => {
        executed.push("c");
        return "ok";
      },
    };
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: ["a", "b", "c"].map((name) => ({
        name,
        description: `Tool ${name}`,
        inputSchema: { type: "object", properties: {} },
        executor: { kind: "native", target: name },
      })),
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("Running batch", [
            { type: "toolCall", id: "tc-a", name: "a", arguments: {} },
            { type: "toolCall", id: "tc-b", name: "b", arguments: {} },
            { type: "toolCall", id: "tc-c", name: "c", arguments: {} },
          ]),
        ],
        usage: emptyUsage,
      })),
    });
    const result = await engine
      .executeStep({
        runId: "test-run",
        stepId: "step-1",
        transcript: [],
        systemPrompt: "test",
        model: makeModel(),
        provider: {},
        settings: makeSettings({
          allowToolCalls: true,
          runtimeLimits: { maxToolCalls: 1 },
          stopConditions: { stopOnToolResult: true },
        }),
      })
      .result();
    // Verify the step ran (messages were produced)
    expect(result.appendedMessages.length).toBeGreaterThanOrEqual(1);
    // Only 1 tool should execute (not 3)
    expect(executed.length).toBe(1);
  });

  it("should return error status on provider failure (fix #3)", async () => {
    const engine = createNativeEngine({
      providerAdapter: makeFauxAdapter(() => ({ messages: [], usage: emptyUsage, isError: true })),
    });
    const result = await engine
      .executeStep({
        runId: "test-run",
        stepId: "step-1",
        transcript: [],
        systemPrompt: "test",
        model: makeModel(),
        provider: {},
        settings: makeSettings({ allowToolCalls: true }),
      })
      .result();
    expect(result.status).toBe("error");
    expect(result.stopReason).toBe("error");
  });
});
