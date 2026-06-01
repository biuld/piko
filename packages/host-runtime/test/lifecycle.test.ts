import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import {
  fauxAssistantMessage,
  fauxThinking,
  fauxToolCall,
  registerFauxProvider,
} from "@earendil-works/pi-ai";
import type { NativeToolRegistry } from "piko-engine-native";
import { createNativeEngine } from "piko-engine-native";

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { HostLifecycleEvent } from "../src/index.js";
import { createHostConfig, PikoHost } from "../src/index.js";

// ============================================================================
// Lifecycle event tests (Phase 1)
// ============================================================================

describe("Lifecycle Events", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-lifecycle",
      models: [{ id: "faux-lifecycle-model" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function lifecycleModel(): Model<string> {
    return {
      id: "faux-lifecycle-model",
      name: "Faux Lifecycle Model",
      api: "openai-completions",
      provider: "faux-lifecycle",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  function collectLifecycle(events: HostLifecycleEvent[]): (e: HostLifecycleEvent) => void {
    return (e) => events.push(e);
  }

  it("should emit agent_start, turn_start, turn_end, save_point, settled, agent_end for a normal run", async () => {
    faux.setResponses([fauxAssistantMessage("Simple reply")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(lifecycleModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt("Hello", { onLifecycleEvent: collectLifecycle(lifecycle) });
    for await (const _event of stream) {
      /* consume engine events */
    }
    await stream.result();

    const types = lifecycle.map((e) => e.type);

    // Verify base lifecycle events are present
    expect(types).toContain("agent_start");
    expect(types).toContain("turn_start");
    expect(types).toContain("turn_end");
    expect(types).toContain("save_point");
    expect(types).toContain("settled");
    expect(types).toContain("agent_end");

    // Verify base events are in correct relative order
    const agentStartIdx = types.indexOf("agent_start");
    const turnStartIdx = types.indexOf("turn_start");
    const turnEndIdx = types.indexOf("turn_end");
    const savePointIdx = types.indexOf("save_point");
    const settledIdx = types.indexOf("settled");
    const agentEndIdx = types.indexOf("agent_end");
    expect(agentStartIdx).toBeLessThan(turnStartIdx);
    expect(turnStartIdx).toBeLessThan(turnEndIdx);
    expect(turnEndIdx).toBeLessThan(savePointIdx);
    expect(savePointIdx).toBeLessThan(settledIdx);
    expect(settledIdx).toBeLessThan(agentEndIdx);

    const agentEnd = lifecycle.find((e) => e.type === "agent_end");
    expect(agentEnd).toMatchObject({ type: "agent_end", status: "completed" });

    const settled = lifecycle.find((e) => e.type === "settled");
    expect(settled).toMatchObject({ type: "settled", nextTurnCount: 0 });
  });

  it("should emit multiple turn_start/turn_end for tool-call continuation", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_lc_1" })]),
      fauxAssistantMessage("Done after tool"),
    ]);

    const toolRegistry: NativeToolRegistry = {
      noop: async () => ({ ok: true }),
    };
    const tools = [
      {
        name: "noop",
        description: "No-op",
        inputSchema: { type: "object", properties: {} },
        executor: { kind: "native" as const, target: "noop" },
      },
    ];

    const host = await PikoHost.create({
      engine: createNativeEngine({ toolRegistry, toolDefinitions: tools }),
      config: createHostConfig(lifecycleModel(), undefined, {
        allowToolCalls: true,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt("Do a tool call", {
      onLifecycleEvent: collectLifecycle(lifecycle),
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    const types = lifecycle.map((e) => e.type);

    // Should have 2 turns (tool call + tool result → continuation)
    const turnStarts = lifecycle.filter((e) => e.type === "turn_start");
    expect(turnStarts).toHaveLength(2);
    if (turnStarts[0]?.type === "turn_start" && turnStarts[1]?.type === "turn_start") {
      expect(turnStarts[0].turnIndex).toBe(0);
      expect(turnStarts[1].turnIndex).toBe(1);
    }

    // Should contain base lifecycle events
    expect(types).toContain("agent_start");
    expect(types).toContain("settled");
    expect(types).toContain("agent_end");

    // Should also contain rich tool execution events
    expect(types).toContain("tool_execution_start");
    expect(types).toContain("tool_execution_end");

    const toolStarts = lifecycle.filter((e) => e.type === "tool_execution_start");
    expect(toolStarts.length).toBeGreaterThan(0);
    const hasNoopStart = toolStarts.some(
      (e) =>
        e.type === "tool_execution_start" && e.toolName === "noop" && e.toolCallId === "call_lc_1",
    );
    expect(hasNoopStart).toBe(true);

    const toolEnds = lifecycle.filter((e) => e.type === "tool_execution_end");
    expect(toolEnds.length).toBeGreaterThan(0);
    const hasNoopEnd = toolEnds.some(
      (e) => e.type === "tool_execution_end" && e.toolName === "noop",
    );
    expect(hasNoopEnd).toBe(true);
  });

  it("should emit queue_update when follow-up messages are consumed", async () => {
    faux.setResponses([
      fauxAssistantMessage("First reply"),
      fauxAssistantMessage("Follow-up reply"),
    ]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(lifecycleModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
        stopConditions: { stopOnAssistantMessage: true },
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    let followUpQueued = false;
    const stream = host.streamPrompt("First prompt", {
      onLifecycleEvent: (e) => {
        lifecycle.push(e);
        if (e.type === "agent_start" && !followUpQueued) {
          followUpQueued = true;
          host.followUp("Continue please");
        }
      },
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    const types = lifecycle.map((e) => e.type);

    expect(types).toContain("queue_update");

    const queueUpdates = lifecycle.filter((e) => e.type === "queue_update");
    expect(queueUpdates.length).toBeGreaterThanOrEqual(1);
  });

  it("should emit queue_update when steering messages are consumed", async () => {
    faux.setResponses([fauxAssistantMessage("Steered reply")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(lifecycleModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    let steerQueued = false;
    const stream = host.streamPrompt("Original prompt", {
      onLifecycleEvent: (e) => {
        lifecycle.push(e);
        if (e.type === "agent_start" && !steerQueued) {
          steerQueued = true;
          host.steer("Steer: use JSON");
        }
      },
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    const types = lifecycle.map((e) => e.type);
    expect(types).toContain("queue_update");
  });

  it("should emit agent_end with max_steps when limit hit", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_ms_1" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_ms_2" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_ms_3" })]),
    ]);

    const toolRegistry: NativeToolRegistry = {
      noop: async () => ({ ok: true }),
    };
    const tools = [
      {
        name: "noop",
        description: "No-op",
        inputSchema: { type: "object", properties: {} },
        executor: { kind: "native" as const, target: "noop" },
      },
    ];

    const host = await PikoHost.create({
      engine: createNativeEngine({ toolRegistry, toolDefinitions: tools }),
      config: createHostConfig(lifecycleModel(), undefined, {
        allowToolCalls: true,
        maxSteps: 2,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt("Loop forever", {
      onLifecycleEvent: collectLifecycle(lifecycle),
    });
    for await (const _event of stream) {
      /* consume */
    }
    const result = await stream.result();

    expect(result.status).toBe("max_steps");

    const types = lifecycle.map((e) => e.type);
    expect(types).toContain("agent_end");

    const agentEnd = lifecycle.find((e) => e.type === "agent_end");
    if (agentEnd?.type === "agent_end") {
      expect(agentEnd.status).toBe("max_steps");
    }
  });

  it("should not emit agent_end on failure (abort/error), only failure + settled", async () => {
    faux.setResponses([fauxAssistantMessage("I will be aborted")]);

    const controller = new AbortController();

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(lifecycleModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt(
      "Abort me",
      { onLifecycleEvent: collectLifecycle(lifecycle) },
      controller.signal,
    );

    controller.abort();

    try {
      for await (const _event of stream) {
        /* consume */
      }
    } catch {
      // Expected on abort
    }

    const types = lifecycle.map((e) => e.type);

    expect(types).toContain("failure");
    expect(types).toContain("settled");
    expect(types).not.toContain("agent_end");

    // Should emit failure message lifecycle before the failure event
    const failureIdx = types.indexOf("failure");
    const msgStartIdx = types.lastIndexOf("message_start");
    const msgEndIdx = types.lastIndexOf("message_end");
    if (msgStartIdx >= 0 && msgEndIdx >= 0) {
      expect(msgStartIdx).toBeLessThan(msgEndIdx);
      expect(msgEndIdx).toBeLessThan(failureIdx);
    }
  });
});

// ============================================================================
// Rich message / tool execution lifecycle tests (Phase 1)
// ============================================================================

describe("Rich Message Lifecycle", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-rich-lifecycle",
      models: [{ id: "faux-rich-model" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function richModel(): Model<string> {
    return {
      id: "faux-rich-model",
      name: "Faux Rich Model",
      api: "openai-completions",
      provider: "faux-rich-lifecycle",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  function collectLifecycle(events: HostLifecycleEvent[]): (e: HostLifecycleEvent) => void {
    return (e) => events.push(e);
  }

  it("should emit message_start, message_update, message_end for assistant message", async () => {
    faux.setResponses([fauxAssistantMessage("A streaming reply")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(richModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt("Hi", {
      onLifecycleEvent: collectLifecycle(lifecycle),
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    const types = lifecycle.map((e) => e.type);
    expect(types).toContain("message_start");
    expect(types).toContain("message_update");
    expect(types).toContain("message_end");

    // Check ordering: message_start before message_update before message_end
    const startIdx = types.indexOf("message_start");
    const updateIdx = types.indexOf("message_update");
    const endIdx = types.indexOf("message_end");
    expect(startIdx).toBeLessThan(updateIdx);
    expect(updateIdx).toBeLessThan(endIdx);

    // message_start should have role "assistant"
    const msgStart = lifecycle.find((e) => e.type === "message_start");
    if (msgStart?.type === "message_start") {
      expect(msgStart.role).toBe("assistant");
      expect(msgStart.messageId).toBeTruthy();
    }

    // message_update should have isThinking: false for regular text
    const msgUpdate = lifecycle.find((e) => e.type === "message_update");
    if (msgUpdate?.type === "message_update") {
      expect(msgUpdate.isThinking).toBe(false);
      expect(msgUpdate.delta).toBeTruthy();
    }

    // message_end should carry a LifecycleMessage
    const msgEnd = lifecycle.find((e) => e.type === "message_end");
    if (msgEnd?.type === "message_end") {
      expect(msgEnd.message.role).toBe("assistant");
    }
  });

  it("should emit message_update with isThinking:true for thinking deltas", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxThinking("Let me think..."), "Thoughtful reply"]),
    ]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(richModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt("Think hard", {
      onLifecycleEvent: collectLifecycle(lifecycle),
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    // Should have at least one thinking update
    const thinkingUpdates = lifecycle.filter(
      (e) => e.type === "message_update" && e.isThinking === true,
    );
    expect(thinkingUpdates.length).toBeGreaterThan(0);
  });

  it("should emit message_start and message_end for steering user messages", async () => {
    faux.setResponses([fauxAssistantMessage("Steered reply")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(richModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    let steered = false;
    const stream = host.streamPrompt("Original", {
      onLifecycleEvent: (e) => {
        lifecycle.push(e);
        if (e.type === "agent_start" && !steered) {
          steered = true;
          host.steer("Use JSON format");
        }
      },
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    // Find user message lifecycle events (from steering)
    const userStarts = lifecycle.filter((e) => e.type === "message_start" && e.role === "user");
    const userEnds = lifecycle.filter((e) => e.type === "message_end" && e.message.role === "user");
    expect(userStarts.length).toBeGreaterThan(0);
    expect(userEnds.length).toBeGreaterThan(0);

    if (userEnds[0]?.type === "message_end") {
      expect(userEnds[0].message.content).toContain("Use JSON format");
    }
  });

  it("should emit message lifecycle for failure messages", async () => {
    faux.setResponses([fauxAssistantMessage("About to fail")]);

    const controller = new AbortController();

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(richModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt(
      "Fail",
      { onLifecycleEvent: collectLifecycle(lifecycle) },
      controller.signal,
    );
    controller.abort();

    try {
      for await (const _event of stream) {
        /* consume */
      }
    } catch {
      // Expected
    }

    // Failure message is an assistant role message
    const failureMsgs = lifecycle.filter(
      (e) => e.type === "message_end" && e.message.role === "assistant",
    );
    // Should have at least one failure message or event
    const hasFailure = lifecycle.some((e) => e.type === "failure");
    const hasFailureContent = failureMsgs.some(
      (e) => e.type === "message_end" && e.message.content.includes("aborted"),
    );
    expect(hasFailure || hasFailureContent).toBe(true);

    // message_start → message_end ordering for failure
    const msgStartIdxs = lifecycle
      .map((e, i) => (e.type === "message_start" ? i : -1))
      .filter((i) => i >= 0);
    const msgEndIdxs = lifecycle
      .map((e, i) => (e.type === "message_end" ? i : -1))
      .filter((i) => i >= 0);

    // Each message_start should have a matching or later message_end
    // (not strictly required for all, but checking the last pair)
    if (msgStartIdxs.length > 0 && msgEndIdxs.length > 0) {
      const lastStart = msgStartIdxs[msgStartIdxs.length - 1];
      const lastEnd = msgEndIdxs[msgEndIdxs.length - 1];
      expect(lastStart).toBeLessThanOrEqual(lastEnd);
    }
  });
});
