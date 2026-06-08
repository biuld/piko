import { describe, expect, it } from "bun:test";
import { createPendingApproval, extractContinuationState } from "../src/approval-state.js";
import { createNativeEngine } from "../src/engine.js";
import { createCounters } from "../src/runtime-limits.js";
import { buildAssistantMessage } from "../src/transcript-builder.js";
import type { NativeToolRegistry } from "../src/types.js";
import { emptyUsage, makeFauxAdapter, makeModel, makeSettings } from "./helpers.js";

describe("Approval Continuation State", () => {
  it("should create pending approval with typed continuation state", () => {
    const continuationState = { version: 1 as const, counters: createCounters() };
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
      kind: "pending_tools" as const,
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

describe("Approval Resolution", () => {
  it("should decline and append a denial tool result", async () => {
    const registry: NativeToolRegistry = { dangerous: async () => "result" };
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
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("Running dangerous tool", [
            { type: "toolCall", id: "tc-1", name: "dangerous", arguments: {} },
          ]),
        ],
        usage: emptyUsage,
      })),
    });

    const result1 = await engine
      .executeStep({
        runId: "test-run",
        stepId: "test-step",
        transcript: [],
        systemPrompt: "test",
        model: makeModel(),
        provider: {},
        settings: makeSettings({ allowToolCalls: true }),
      })
      .result();
    expect(result1.status).toBe("awaiting_approval");

    if (engine.resolveApproval) {
      const r = await engine.resolveApproval({
        runId: "test-run",
        stepId: "test-step",
        approvalRequestId: result1.pendingApproval!.requestId,
        decision: "decline",
        transcript: result1.appendedMessages,
        engineState: result1.engineState,
      });
      expect(r.status).toBe("completed");
      const hasDenial = r.appendedMessages.some(
        (m) =>
          m.role === "toolResult" &&
          Array.isArray(m.content) &&
          m.content.some((c) => c.type === "text" && c.text.includes("declined")),
      );
      expect(hasDenial).toBe(true);
    }
  });

  it("should accept and resume tool execution", async () => {
    const registry: NativeToolRegistry = { echo: async (args) => args.message ?? "" };
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "echo",
          description: "Echo tool",
          inputSchema: { type: "object", properties: { message: { type: "string" } } },
          executor: { kind: "native", target: "echo" },
          metadata: { requiresApproval: true },
        },
      ],
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("Echo test", [
            { type: "toolCall", id: "tc-1", name: "echo", arguments: { message: "hi" } },
          ]),
        ],
        usage: emptyUsage,
      })),
    });

    const result1 = await engine
      .executeStep({
        runId: "test-run",
        stepId: "test-step",
        transcript: [],
        systemPrompt: "test",
        model: makeModel(),
        provider: {},
        settings: makeSettings({ allowToolCalls: true }),
      })
      .result();
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
      expect(result2.appendedMessages.some((m) => m.role === "toolResult")).toBe(true);
    }
  });

  it("should preserve executorTarget in approval resume", async () => {
    // Tool named "dangerous" but executor targets "bash" (alias)
    const registry: NativeToolRegistry = {
      bash: async () => "bash result",
      dangerous: async () => "dangerous result",
    };
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
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("Running dangerous", [
            { type: "toolCall", id: "tc-1", name: "dangerous", arguments: {} },
          ]),
        ],
        usage: emptyUsage,
      })),
    });

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

    if (engine.resolveApproval) {
      const result2 = await engine.resolveApproval({
        runId: "test-run",
        stepId: "step-1",
        approvalRequestId: result1.pendingApproval!.requestId,
        decision: "accept",
        transcript: result1.appendedMessages,
        engineState: result1.engineState,
      });
      const toolMsg = result2.appendedMessages.find((m) => m.role === "toolResult");
      expect(toolMsg).toBeDefined();
      expect(toolMsg!.isError).toBe(false);
      // Verify it called "bash" executor (correct target), not "dangerous"
      const detailStr = JSON.stringify(toolMsg!.details);
      expect(detailStr).toContain("bash result");
    }
  });

  it("should preserve runtime settings through approval resume", async () => {
    const registry: NativeToolRegistry = {
      slow: async () => {
        await new Promise((r) => setTimeout(r, 100));
        return "done";
      },
    };
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
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("Running slow tool", [
            { type: "toolCall", id: "tc-1", name: "slow", arguments: {} },
          ]),
        ],
        usage: emptyUsage,
      })),
    });

    const result1 = await engine
      .executeStep({
        runId: "test-run",
        stepId: "step-1",
        transcript: [],
        systemPrompt: "test",
        model: makeModel(),
        provider: {},
        settings: makeSettings({ allowToolCalls: true, runtimeLimits: { perToolTimeoutMs: 200 } }),
      })
      .result();
    expect(result1.status).toBe("awaiting_approval");

    // Verify the pending state carries runtimeLimits
    const cs = result1.engineState as {
      version: number;
      pendingToolCalls?: { settings?: { runtimeLimits?: { perToolTimeoutMs?: number } } };
    };
    expect(cs.pendingToolCalls?.settings?.runtimeLimits?.perToolTimeoutMs).toBe(200);

    if (engine.resolveApproval) {
      const result2 = await engine.resolveApproval({
        runId: "test-run",
        stepId: "step-1",
        approvalRequestId: result1.pendingApproval!.requestId,
        decision: "accept",
        transcript: result1.appendedMessages,
        engineState: result1.engineState,
      });
      const toolMsg = result2.appendedMessages.find((m) => m.role === "toolResult");
      expect(toolMsg).toBeDefined();
      expect(toolMsg!.isError).toBe(false);
    }
  });

  it("should append tool results executed before an approval pause", async () => {
    const registry: NativeToolRegistry = {
      safe: async () => "safe result",
      dangerous: async () => "dangerous result",
    };
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "safe",
          description: "Safe",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "safe" },
        },
        {
          name: "dangerous",
          description: "Dangerous",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "dangerous" },
          metadata: { requiresApproval: true },
        },
      ],
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("Run both", [
            { type: "toolCall", id: "tc-safe", name: "safe", arguments: {} },
            { type: "toolCall", id: "tc-danger", name: "dangerous", arguments: {} },
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
        settings: makeSettings({ allowToolCalls: true }),
      })
      .result();

    expect(result.status).toBe("awaiting_approval");
    const safeResult = result.appendedMessages.find(
      (m) => m.role === "toolResult" && m.toolCallId === "tc-safe",
    );
    expect(safeResult).toBeDefined();
    expect(JSON.stringify(safeResult!.details)).toContain("safe result");
  });

  it("should request approval again for later approval-required pending tools", async () => {
    const registry: NativeToolRegistry = {
      first: async () => "first result",
      second: async () => "second result",
    };
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "first",
          description: "First",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "first" },
          metadata: { requiresApproval: true },
        },
        {
          name: "second",
          description: "Second",
          inputSchema: { type: "object", properties: {} },
          executor: { kind: "native", target: "second" },
          metadata: { requiresApproval: true },
        },
      ],
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("Run both dangerous tools", [
            { type: "toolCall", id: "tc-first", name: "first", arguments: {} },
            { type: "toolCall", id: "tc-second", name: "second", arguments: {} },
          ]),
        ],
        usage: emptyUsage,
      })),
    });

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
    expect(result1.pendingApproval.requestId).toBe("tc-first");

    const result2 = await engine.resolveApproval!({
      runId: "test-run",
      stepId: "step-1",
      approvalRequestId: result1.pendingApproval.requestId,
      decision: "accept",
      transcript: result1.appendedMessages,
      engineState: result1.engineState,
    });

    expect(result2.status).toBe("awaiting_approval");
    expect(result2.pendingApproval.requestId).toBe("tc-second");
    expect(
      result2.appendedMessages.some((m) => m.role === "toolResult" && m.toolCallId === "tc-first"),
    ).toBe(true);
    expect(
      result2.appendedMessages.some((m) => m.role === "toolResult" && m.toolCallId === "tc-second"),
    ).toBe(false);
  });
});
