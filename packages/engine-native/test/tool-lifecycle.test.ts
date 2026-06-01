import { describe, expect, it } from "bun:test";
import type { EngineInput } from "piko-engine-protocol";
import { createNativeEngine } from "../src/engine.js";
import { buildAssistantMessage } from "../src/transcript-builder.js";
import type { NativeToolRegistry } from "../src/types.js";
import { collectEvents, emptyUsage, makeFauxAdapter, makeModel, makeSettings } from "./helpers.js";

describe("Tool Lifecycle", () => {
  it("should handle unknown tool as tool-result error", async () => {
    const registry: NativeToolRegistry = {};
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [],
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("test", [
            { type: "toolCall", id: "tc-1", name: "nonexistent", arguments: { foo: "bar" } },
          ]),
        ],
        usage: emptyUsage,
      })),
    });
    const input: EngineInput = {
      runId: "test-run",
      stepId: "test-step",
      transcript: [],
      systemPrompt: "You are a test assistant",
      model: makeModel(),
      provider: {},
      settings: makeSettings({ allowToolCalls: true, stopConditions: { stopOnToolResult: true } }),
    };
    const result = await engine.executeStep(input).result();
    const toolResult = result.appendedMessages.find((m) => m.role === "toolResult");
    expect(toolResult).toBeDefined();
    expect(toolResult!.isError).toBe(true);
  });

  it("should emit tool_call_skipped for approval-required tools", async () => {
    const registry: NativeToolRegistry = { dangerous: async () => "done" };
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
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("I'll run a dangerous tool", [
            { type: "toolCall", id: "tc-1", name: "dangerous", arguments: {} },
          ]),
        ],
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
      settings: makeSettings({ allowToolCalls: true }),
    };
    const stream = engine.executeStep(input);
    const events = await collectEvents(stream);
    const result = await stream.result();
    expect(events.find((e) => e.type === "approval_requested")).toBeDefined();
    expect(result.status).toBe("awaiting_approval");
    expect(result.pendingApproval).toBeDefined();
  });
});

describe("Transcript Delta", () => {
  it("should include assistant_message and tool_result deltas", async () => {
    const registry: NativeToolRegistry = { echo: async (args) => args.message ?? "" };
    const engine = createNativeEngine({
      toolRegistry: registry,
      toolDefinitions: [
        {
          name: "echo",
          description: "Echo",
          inputSchema: { type: "object", properties: { message: { type: "string" } } },
          executor: { kind: "native", target: "echo" },
        },
      ],
      providerAdapter: makeFauxAdapter(() => ({
        messages: [
          buildAssistantMessage("I'll echo", [
            { type: "toolCall", id: "tc-echo", name: "echo", arguments: { message: "hello" } },
          ]),
        ],
        usage: emptyUsage,
      })),
    });
    const result = await engine
      .executeStep({
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
      })
      .result();
    expect(result.transcriptDelta).toBeDefined();
    if (result.transcriptDelta) {
      const kinds = result.transcriptDelta.map((d) => d.kind);
      expect(kinds).toContain("assistant_message");
      expect(kinds).toContain("tool_result");
    }
  });
});

describe("Tool Event Deduplication", () => {
  it("should not emit duplicate tool_call_end events", async () => {
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
          buildAssistantMessage("echo test", [
            { type: "toolCall", id: "tc-1", name: "echo", arguments: {} },
          ]),
        ],
        usage: emptyUsage,
      })),
    });
    const stream = engine.executeStep({
      runId: "test-run",
      stepId: "step-1",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({ allowToolCalls: true, stopConditions: { stopOnToolResult: true } }),
    });
    const events = await collectEvents(stream);
    expect(events.filter((e) => e.type === "tool_call_start" && e.id === "tc-1").length).toBe(1);
    expect(events.filter((e) => e.type === "tool_call_end" && e.id === "tc-1").length).toBe(1);
  });

  it("should forward normalized provider events in engine event stream", async () => {
    const engine = createNativeEngine({
      providerAdapter: makeFauxAdapter(() => ({
        messages: [buildAssistantMessage("hello", [])],
        usage: emptyUsage,
      })),
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
    expect(events.find((e) => e.type === "provider_request_start")).toBeDefined();
    expect(events.find((e) => e.type === "provider_message_end")).toBeDefined();
  });
});
