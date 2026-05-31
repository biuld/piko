import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, fauxToolCall, registerFauxProvider } from "@earendil-works/pi-ai";
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

    expect(types).toEqual([
      "agent_start",
      "turn_start",
      "turn_end",
      "save_point",
      "settled",
      "agent_end",
    ]);

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

    expect(types).toEqual([
      "agent_start",
      "turn_start",
      "turn_end",
      "save_point",
      "turn_start",
      "turn_end",
      "save_point",
      "settled",
      "agent_end",
    ]);

    const turnStarts = lifecycle.filter((e) => e.type === "turn_start");
    expect(turnStarts).toHaveLength(2);
    if (turnStarts[0]?.type === "turn_start" && turnStarts[1]?.type === "turn_start") {
      expect(turnStarts[0].turnIndex).toBe(0);
      expect(turnStarts[1].turnIndex).toBe(1);
    }
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
  });
});
