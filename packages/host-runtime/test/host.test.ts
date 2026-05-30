import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { registerFauxProvider, fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import type { FauxProviderRegistration } from "@earendil-works/pi-ai";
import { createNativeEngine } from "piko-engine-native";
import type { NativeToolRegistry } from "piko-engine-native";
import type { EngineModel } from "piko-engine-protocol";
import { PikoHost, createHostConfig, createPiLlmCaller } from "../src/index.js";

const PROVIDER = "faux";
const API = "openai-completions";
const MODEL_ID = "faux-host-model";

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

function buildTestModel(): EngineModel {
  return {
    id: MODEL_ID,
    name: "Faux Host Model",
    api: API,
    provider: PROVIDER,
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

function buildTestTool(name: string, description: string, inputSchema?: Record<string, unknown>) {
  return {
    name,
    description,
    inputSchema: inputSchema ?? { type: "object", properties: {} },
    executor: { kind: "native" as const, target: name },
  };
}

describe("PikoHost", () => {
  it("should run a simple prompt and return assistant response", async () => {
    faux.setResponses([fauxAssistantMessage("Hello! How can I help?")]);

    const engine = createNativeEngine({ llmCaller: createPiLlmCaller() });
    const config = createHostConfig(buildTestModel());

    const host = new PikoHost({ engine, config });

    const result = await host.run("Hi there");

    expect(result.status).toBe("completed");
    expect(result.messages.length).toBeGreaterThanOrEqual(2);

    const userMsg = result.messages[0];
    expect(userMsg.role).toBe("user");

    const assistantMsgs = result.messages.filter((m) => m.role === "assistant");
    expect(assistantMsgs.length).toBeGreaterThan(0);
  });

  it("should handle tool calls", async () => {
    faux.setResponses([
      fauxAssistantMessage([
        fauxToolCall("echo", { text: "hello" }, { id: "call_echo" }),
      ]),
    ]);

    const toolRegistry: NativeToolRegistry = {
      echo: async (args) => {
        return { echoed: args.text };
      },
    };

    const engine = createNativeEngine({ llmCaller: createPiLlmCaller(), tools: toolRegistry });
    const config = createHostConfig(buildTestModel());
    const tools = [buildTestTool("echo", "Echoes back the text", {
      type: "object",
      properties: { text: { type: "string" } },
    })];

    const host = new PikoHost({ engine, config, tools });

    const result = await host.run("Echo hello");

    expect(result.status).toBe("completed");
    expect(result.totalSteps).toBeGreaterThanOrEqual(1);

    const toolMsgs = result.messages.filter((m) => m.role === "toolResult");
    expect(toolMsgs.length).toBeGreaterThan(0);

    if (toolMsgs.length > 0 && toolMsgs[0].role === "toolResult") {
      expect(toolMsgs[0].toolName).toBe("echo");
      expect(toolMsgs[0].isError).toBe(false);
    }
  });

  it("should stop after max steps", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_noop_1" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_noop_2" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_noop_3" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_noop_4" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_noop_5" })]),
    ]);

    const toolRegistry: NativeToolRegistry = {
      noop: async () => ({ ok: true }),
    };

    const engine = createNativeEngine({ llmCaller: createPiLlmCaller(), tools: toolRegistry });
    const config = createHostConfig(buildTestModel(), undefined, { maxSteps: 3 });
    const tools = [buildTestTool("noop", "No operation")];

    const host = new PikoHost({ engine, config, tools });

    const result = await host.run("Loop test");

    expect(result.status).toBe("max_steps");
    expect(result.totalSteps).toBe(3);
  });
});
