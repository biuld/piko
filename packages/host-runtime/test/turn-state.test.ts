import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, fauxToolCall, registerFauxProvider } from "@earendil-works/pi-ai";
import type { NativeToolRegistry } from "piko-engine-native";
import { createNativeEngine } from "piko-engine-native";

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { HostLifecycleEvent } from "../src/index.js";
import { createHostConfig, PikoHost } from "../src/index.js";

// ============================================================================
// Turn State / prepareNextTurn tests (Phase 2)
// ============================================================================

describe("Turn State (prepareNextTurn)", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-turnstate",
      models: [{ id: "model-a" }, { id: "model-b" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function tsModel(id: string): Model<string> {
    return {
      id,
      name: `Model ${id}`,
      api: "openai-completions",
      provider: "faux-turnstate",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  it("should pick up model change mid-run via dynamic prepareTurn", async () => {
    const modelA = tsModel("model-a");
    const modelB = tsModel("model-b");

    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_ts_1" })]),
      fauxAssistantMessage("Done after switch"),
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
      config: createHostConfig(modelA, undefined, {
        allowToolCalls: true,
        maxSteps: 5,
      }),
    });

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt("Switch models mid-run", {
      onLifecycleEvent: (e) => {
        lifecycle.push(e);
        if (e.type === "turn_end" && e.turnIndex === 0) {
          host.setConfig(
            createHostConfig(modelB, undefined, {
              allowToolCalls: true,
              maxSteps: 5,
            }),
          );
        }
      },
    });

    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    const turnStarts = lifecycle.filter((e) => e.type === "turn_start");
    expect(turnStarts).toHaveLength(2);

    expect(host.getConfig().model.id).toBe("model-b");
  });

  it("should pick up thinking level change mid-run", async () => {
    const model = tsModel("model-a");

    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_think_1" })]),
      fauxAssistantMessage("Thinking switched"),
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
      config: createHostConfig(model, undefined, {
        allowToolCalls: true,
        maxSteps: 5,
      }),
    });

    expect(host.getThinkingLevel()).toBe("off");

    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt("Change thinking mid-run", {
      onLifecycleEvent: (e) => {
        lifecycle.push(e);
        if (e.type === "turn_end" && e.turnIndex === 0) {
          host.setThinkingLevel("medium");
        }
      },
    });

    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    expect(host.getThinkingLevel()).toBe("medium");
    expect(lifecycle.filter((e) => e.type === "turn_start")).toHaveLength(2);
  });

  it("should pass correct TurnContext to prepareTurn", async () => {
    const model = tsModel("model-a");

    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_ctx_1" })]),
      fauxAssistantMessage("Second turn done"),
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

    const engine = createNativeEngine({ toolRegistry, toolDefinitions: tools });
    const host = await PikoHost.create({
      engine,
      config: createHostConfig(model, undefined, {
        allowToolCalls: true,
        maxSteps: 5,
      }),
    });

    const stream = host.streamPrompt("Test turn context");
    for await (const _event of stream) {
      /* consume */
    }
    const result = await stream.result();

    expect(result.status).toBe("completed");
    expect(result.messages.length).toBeGreaterThanOrEqual(4);
  });
});
