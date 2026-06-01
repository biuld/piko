import type {
  EngineEvent,
  EngineInput,
  EngineRunSettings,
  EventStream,
  Model,
  TokenUsage,
} from "piko-engine-protocol";
import { describe, expect, it } from "vitest";
import { createPendingApproval, extractContinuationState } from "../src/approval-state.js";
import { createNativeEngine } from "../src/engine.js";
import type {
  ProviderAdapter,
  ProviderAdapterResult,
  ProviderContext,
} from "../src/provider/types.js";
import {
  checkBeforeApproval,
  checkBeforeModelCall,
  checkBeforeToolCall,
  createCounters,
  withToolTimeout,
} from "../src/runtime-limits.js";
import { buildAssistantMessage } from "../src/transcript-builder.js";
import type { NativeToolRegistry } from "../src/types.js";

// ---- FauxProvider Adapter ----

const emptyUsage: TokenUsage = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
};

function makeModel(): Model<string> {
  return {
    id: "test-model",
    name: "Test Model",
    api: "openai-completions" as const,
    provider: "openai",
    baseUrl: "https://api.openai.com/v1",
    reasoning: false,
  };
}

function makeSettings(overrides?: Partial<EngineRunSettings>): EngineRunSettings {
  return {
    maxSteps: 10,
    allowToolCalls: true,
    allowApprovals: true,
    ...overrides,
  };
}

function makeFauxAdapter(
  handler: (context: ProviderContext) => ProviderAdapterResult,
): ProviderAdapter {
  return {
    async stream(_model, context, _options, emit): Promise<ProviderAdapterResult> {
      emit({
        type: "provider_request_start",
        provider: "test",
        model: "test-model",
      });
      const result = handler(context);
      const msg = result.messages[0] ?? {
        role: "assistant" as const,
        content: [],
        api: "openai-completions" as const,
        provider: "test",
        model: "test-model",
        usage: emptyUsage,
        stopReason: "stop" as const,
        timestamp: Date.now(),
      };
      emit({
        type: "provider_message_end",
        message: msg,
        usage: result.usage,
      });
      return { isError: false, ...result };
    },
  };
}

function collectEvents(stream: EventStream<EngineEvent, unknown>): Promise<EngineEvent[]> {
  const events: EngineEvent[] = [];
  return new Promise((resolve, reject) => {
    void (async () => {
      try {
        for await (const event of stream) {
          events.push(event);
        }
        resolve(events);
      } catch (err) {
        reject(err);
      }
    })();
  });
}

// ---- Tests ----

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
    const result = checkBeforeModelCall(counters, {
      maxConsecutiveErrors: 5,
    });
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

describe("Approval Continuation State", () => {
  it("should create pending approval with typed continuation state", () => {
    const continuationState = {
      version: 1 as const,
      counters: createCounters(),
    };
    const approval = createPendingApproval(
      { requestId: "req-1", kind: "tool:edit", details: {} },
      continuationState,
    );
    expect(approval.requestId).toBe("req-1");
    expect(approval.engineState).toEqual(continuationState);
  });

  it("should extract typed continuation state", () => {
    const cs = {
      version: 1 as const,
      pendingToolCalls: {
        assistantMessage: buildAssistantMessage("test", []),
        remainingToolCallIds: ["tc-1"],
        toolCalls: [{ id: "tc-1", name: "read", args: { path: "/test" } }],
        settings: { parallelTools: true },
      },
      counters: createCounters(),
    };
    const approval = createPendingApproval(
      { requestId: "req-1", kind: "tool:edit", details: {} },
      cs,
    );
    const extracted = extractContinuationState({
      runId: "run-1",
      stepId: "step-1",
      approvalRequestId: "req-1",
      decision: "accept",
      transcript: [],
      engineState: approval.engineState,
    });
    expect(extracted).toBeDefined();
    expect(extracted!.version).toBe(1);
    expect(extracted!.pendingToolCalls?.remainingToolCallIds).toEqual(["tc-1"]);
  });
});

describe("Tool Lifecycle", () => {
  it("should handle unknown tool as tool-result error", async () => {
    const registry: NativeToolRegistry = {};
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("test", [
          {
            type: "toolCall",
            id: "tc-1",
            name: "nonexistent",
            arguments: { foo: "bar" },
          },
        ]),
      ],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [],
      providerAdapter: fauxAdapter,
    });

    const input: EngineInput = {
      runId: "test-run",
      stepId: "test-step",
      transcript: [],
      systemPrompt: "You are a test assistant",
      model: makeModel(),
      provider: {},
      settings: makeSettings({
        allowToolCalls: true,
        stopConditions: { stopOnToolResult: true },
      }),
    };

    const result = await engine.executeStep(input).result();
    const toolResult = result.appendedMessages.find((m) => m.role === "toolResult");
    expect(toolResult).toBeDefined();
    expect(toolResult!.isError).toBe(true);
  });

  it("should emit tool_call_skipped for approval-required tools", async () => {
    const registry: NativeToolRegistry = {
      dangerous: async () => "done",
    };
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("I'll run a dangerous tool", [
          {
            type: "toolCall",
            id: "tc-1",
            name: "dangerous",
            arguments: {},
          },
        ]),
      ],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "dangerous",
          description: "A dangerous tool",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "dangerous" },
          metadata: { requiresApproval: true },
        },
      ],
      providerAdapter: fauxAdapter,
    });

    const input: EngineInput = {
      runId: "test-run",
      stepId: "test-step",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({ allowToolCalls: true }),
    };

    const stream = engine.executeStep(input);
    const events = await collectEvents(stream);
    const result = await stream.result();

    const approvalEvent = events.find((e) => e.type === "approval_requested");
    expect(approvalEvent).toBeDefined();
    expect(result.status).toBe("awaiting_approval");
    expect(result.pendingApproval).toBeDefined();
  });
});

describe("Transcript Delta", () => {
  it("should include assistant_message and tool_result deltas", async () => {
    const registry: NativeToolRegistry = {
      echo: async (args) => args.message ?? "",
    };
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("I'll echo", [
          {
            type: "toolCall",
            id: "tc-echo",
            name: "echo",
            arguments: { message: "hello" },
          },
        ]),
      ],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "echo",
          description: "Echo",
          inputSchema: {
            type: "object",
            properties: { message: { type: "string" } },
          },
          executor: { kind: "native", target: "echo" },
        },
      ],
      providerAdapter: fauxAdapter,
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
        stopConditions: { stopOnToolResult: true },
      }),
    };

    const result = await engine.executeStep(input).result();
    expect(result.transcriptDelta).toBeDefined();
    if (result.transcriptDelta) {
      const kinds = result.transcriptDelta.map((d) => d.kind);
      expect(kinds).toContain("assistant_message");
      expect(kinds).toContain("tool_result");
    }
  });
});

describe("Approval Resolution", () => {
  it("should decline and append a denial tool result", async () => {
    const registry: NativeToolRegistry = {
      dangerous: async () => "result",
    };
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("Running dangerous tool", [
          {
            type: "toolCall",
            id: "tc-1",
            name: "dangerous",
            arguments: {},
          },
        ]),
      ],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "dangerous",
          description: "Dangerous",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "dangerous" },
          metadata: { requiresApproval: true },
        },
      ],
      providerAdapter: fauxAdapter,
    });

    const input: EngineInput = {
      runId: "test-run",
      stepId: "test-step",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({ allowToolCalls: true }),
    };

    const result1 = await engine.executeStep(input).result();
    expect(result1.status).toBe("awaiting_approval");

    if (engine.resolveApproval) {
      const resolutionResult = await engine.resolveApproval({
        runId: "test-run",
        stepId: "test-step",
        approvalRequestId: result1.pendingApproval!.requestId,
        decision: "decline",
        transcript: result1.appendedMessages,
        engineState: result1.engineState,
      });
      expect(resolutionResult.status).toBe("completed");
      const hasDenial = resolutionResult.appendedMessages.some(
        (m) =>
          m.role === "toolResult" &&
          Array.isArray(m.content) &&
          m.content.some((c) => c.type === "text" && c.text.includes("declined")),
      );
      expect(hasDenial).toBe(true);
    }
  });

  it("should accept and resume tool execution", async () => {
    const registry: NativeToolRegistry = {
      echo: async (args) => args.message ?? "",
    };
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("Echo test", [
          {
            type: "toolCall",
            id: "tc-1",
            name: "echo",
            arguments: { message: "hi" },
          },
        ]),
      ],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "echo",
          description: "Echo tool",
          inputSchema: {
            type: "object",
            properties: { message: { type: "string" } },
          },
          executor: { kind: "native", target: "echo" },
          metadata: { requiresApproval: true },
        },
      ],
      providerAdapter: fauxAdapter,
    });

    const input: EngineInput = {
      runId: "test-run",
      stepId: "test-step",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({ allowToolCalls: true }),
    };

    const result1 = await engine.executeStep(input).result();
    expect(result1.status).toBe("awaiting_approval");

    if (engine.resolveApproval) {
      const result2 = await engine.resolveApproval({
        runId: "test-run",
        stepId: "test-step",
        approvalRequestId: result1.pendingApproval!.requestId,
        decision: "accept",
        transcript: result1.appendedMessages,
        engineState: result1.engineState,
      });
      expect(result2.status).toBe("continue");
      const hasToolResult = result2.appendedMessages.some((m) => m.role === "toolResult");
      expect(hasToolResult).toBe(true);
    }
  });
});

describe("Engine Step with Runtime Limits", () => {
  it("should stop when max model calls is reached", async () => {
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [buildAssistantMessage("hello", [])],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      providerAdapter: fauxAdapter,
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
});

describe("Engine Continuation State", () => {
  it("should preserve counters across steps", async () => {
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [buildAssistantMessage("all done", [])],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      providerAdapter: fauxAdapter,
    });

    const input: EngineInput = {
      runId: "test-run",
      stepId: "step-1",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({
        allowToolCalls: true,
        stopConditions: { stopOnAssistantMessage: true },
      }),
    };

    const result = await engine.executeStep(input).result();
    expect(result.engineState).toBeDefined();

    const cs = result.engineState as {
      version: number;
      counters?: { modelCalls: number };
    };
    expect(cs.version).toBe(1);
    expect(cs.counters?.modelCalls).toBeGreaterThanOrEqual(1);
  });
});

describe("Bug fixes", () => {
  it("should preserve executorTarget in approval resume (fix #1)", async () => {
    // Tool named "dangerous" but executor targets "bash" (alias)
    // The registry must include both the display name and the target key
    const registry: NativeToolRegistry = {
      bash: async () => "bash result",
      dangerous: async () => "dangerous result",
    };
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("Running dangerous", [
          {
            type: "toolCall",
            id: "tc-1",
            name: "dangerous",
            arguments: {},
          },
        ]),
      ],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "dangerous",
          description: "Dangerous (aliased to bash)",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "bash" },
          metadata: { requiresApproval: true },
        },
      ],
      providerAdapter: fauxAdapter,
    });

    // Step 1: Get approval request
    const result1 = await engine
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
    expect(result1.status).toBe("awaiting_approval");

    // Step 2: Accept — should call "bash" executor, not "dangerous"
    if (engine.resolveApproval) {
      const result2 = await engine.resolveApproval({
        runId: "test-run",
        stepId: "step-1",
        approvalRequestId: result1.pendingApproval!.requestId,
        decision: "accept",
        transcript: result1.appendedMessages,
        engineState: result1.engineState,
      });
      // Should have a successful tool result (not "No executor registered")
      const toolMsg = result2.appendedMessages.find((m) => m.role === "toolResult");
      expect(toolMsg).toBeDefined();
      expect(toolMsg!.isError).toBe(false);
      // Verify it called "bash" executor (correct target), not "dangerous"
      const detailStr = JSON.stringify(toolMsg!.details);
      expect(detailStr).toContain("bash result");
    }
  });

  it("should enforce maxToolCalls limit (fix #2)", async () => {
    const registry: NativeToolRegistry = {
      echo: async () => "ok",
    };
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("echoing", [
          {
            type: "toolCall",
            id: "tc-1",
            name: "echo",
            arguments: {},
          },
        ]),
      ],
      usage: emptyUsage,
    }));
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
      providerAdapter: fauxAdapter,
    });

    // Start with counters already at max
    const input: EngineInput = {
      runId: "test-run",
      stepId: "step-1",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({
        allowToolCalls: true,
        runtimeLimits: { maxToolCalls: 0 },
      }),
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
    // Should stop before executing tools because maxToolCalls is hit
    expect(result.stopReason).toBe("max_steps");
    // Should still have the assistant message but no tool results
    expect(result.appendedMessages.length).toBe(1);
    expect(result.appendedMessages[0].role).toBe("assistant");
  });

  it("should return error status on provider failure (fix #3)", async () => {
    // Adapter that returns no messages (simulates provider failure)
    const failingAdapter = makeFauxAdapter(() => ({
      messages: [],
      usage: emptyUsage,
      isError: true,
    }));
    const engine = createNativeEngine({
      providerAdapter: failingAdapter,
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

    // Provider failure should result in error status, not completed
    expect(result.status).toBe("error");
    expect(result.stopReason).toBe("error");
  });

  it("should limit tool calls within a single batch (fix #1)", async () => {
    // maxToolCalls: 1 but assistant returns 3 → only 1 should execute
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
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("Running batch", [
          { type: "toolCall", id: "tc-a", name: "a", arguments: {} },
          { type: "toolCall", id: "tc-b", name: "b", arguments: {} },
          { type: "toolCall", id: "tc-c", name: "c", arguments: {} },
        ]),
      ],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: ["a", "b", "c"].map((name) => ({
        name,
        description: `Tool ${name}`,
        inputSchema: { type: "object", properties: {} },
        executor: { kind: "native", target: name },
      })),
      providerAdapter: fauxAdapter,
    });

    const input: EngineInput = {
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
    };

    const _result = await engine.executeStep(input).result();
    // Only 1 tool should execute (not 3)
    expect(executed.length).toBe(1);
  });

  it("should not emit duplicate tool_call_end events (fix #2)", async () => {
    const registry: NativeToolRegistry = {
      echo: async () => "ok",
    };
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("echo test", [
          { type: "toolCall", id: "tc-1", name: "echo", arguments: {} },
        ]),
      ],
      usage: emptyUsage,
    }));
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
      providerAdapter: fauxAdapter,
    });

    const stream = engine.executeStep({
      runId: "test-run",
      stepId: "step-1",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({
        allowToolCalls: true,
        stopConditions: { stopOnToolResult: true },
      }),
    });
    const events = await collectEvents(stream);

    // Count tool_call_start events — should be exactly 1 per tool call
    const startCount = events.filter((e) => e.type === "tool_call_start" && e.id === "tc-1").length;
    expect(startCount).toBe(1);

    // Count tool_call_end events — should be exactly 1 per tool call
    const endCount = events.filter((e) => e.type === "tool_call_end" && e.id === "tc-1").length;
    expect(endCount).toBe(1);
  });

  it("should preserve runtime settings through approval resume (fix #4)", async () => {
    // Test that perToolTimeoutMs is present in pending state and applied on resume
    const registry: NativeToolRegistry = {
      slow: async () => {
        await new Promise((r) => setTimeout(r, 100));
        return "done";
      },
    };
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [
        buildAssistantMessage("Running slow tool", [
          {
            type: "toolCall",
            id: "tc-1",
            name: "slow",
            arguments: {},
          },
        ]),
      ],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "slow",
          description: "Slow tool",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "slow" },
          metadata: { requiresApproval: true },
        },
      ],
      providerAdapter: fauxAdapter,
    });

    // Step 1: Get approval request
    const result1 = await engine
      .executeStep({
        runId: "test-run",
        stepId: "step-1",
        transcript: [],
        systemPrompt: "test",
        model: makeModel(),
        provider: {},
        settings: makeSettings({
          allowToolCalls: true,
          runtimeLimits: { perToolTimeoutMs: 200 },
        }),
      })
      .result();
    expect(result1.status).toBe("awaiting_approval");

    // Verify the pending state carries runtimeLimits
    const cs = result1.engineState as {
      version: number;
      pendingToolCalls?: {
        settings?: { runtimeLimits?: { perToolTimeoutMs?: number } };
      };
    };
    expect(cs.pendingToolCalls?.settings?.runtimeLimits?.perToolTimeoutMs).toBe(200);

    // Step 2: Accept — tool should complete within the 200ms timeout
    if (engine.resolveApproval) {
      const result2 = await engine.resolveApproval({
        runId: "test-run",
        stepId: "step-1",
        approvalRequestId: result1.pendingApproval!.requestId,
        decision: "accept",
        transcript: result1.appendedMessages,
        engineState: result1.engineState,
      });
      // Tool should succeed (100ms < 200ms timeout)
      const toolMsg = result2.appendedMessages.find((m) => m.role === "toolResult");
      expect(toolMsg).toBeDefined();
      expect(toolMsg!.isError).toBe(false);
    }
  });

  it("should forward normalized provider events (fix #4)", async () => {
    const fauxAdapter = makeFauxAdapter(() => ({
      messages: [buildAssistantMessage("hello", [])],
      usage: emptyUsage,
    }));
    const engine = createNativeEngine({
      providerAdapter: fauxAdapter,
    });

    const stream = engine.executeStep({
      runId: "test-run",
      stepId: "step-1",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({
        allowToolCalls: true,
        stopConditions: { stopOnAssistantMessage: true },
      }),
    });
    const events = await collectEvents(stream);

    // Should include normalized provider events
    const requestStart = events.find((e) => e.type === "provider_request_start");
    expect(requestStart).toBeDefined();

    const messageEnd = events.find((e) => e.type === "provider_message_end");
    expect(messageEnd).toBeDefined();
  });
});
