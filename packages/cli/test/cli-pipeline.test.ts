import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { registerFauxProvider, fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import type { FauxProviderRegistration } from "@earendil-works/pi-ai";
import { createNativeEngine } from "piko-engine-native";
import type { NativeToolRegistry } from "piko-engine-native";
import { PikoHost, createHostConfig, createDefaultSettings } from "piko-host-runtime";
import type { EngineModel, EngineProviderConfig } from "piko-engine-protocol";

const PROVIDER = "faux";
const API = "openai-completions";
const MODEL_ID = "faux-cli-model";

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
    name: "Faux CLI Model",
    api: API,
    provider: PROVIDER,
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

function buildProviderConfig(): EngineProviderConfig {
  return {};
}

describe("CLI pipeline", () => {
  it("full pipeline: engine + host run a prompt", async () => {
    faux.setResponses([fauxAssistantMessage("Hello from piko!")]);

    const engine = createNativeEngine();
    const config = createHostConfig(
      buildTestModel(),
      buildProviderConfig(),
      createDefaultSettings({ allowToolCalls: false, maxSteps: 1 }),
    );

    const host = new PikoHost({
      engine,
      config,
      systemPrompt: "You are a helpful assistant.",
    });

    const result = await host.run("Hi");

    expect(result.status).toBe("completed");
    const assistantMsgs = result.messages.filter((m) => m.role === "assistant");
    expect(assistantMsgs.length).toBeGreaterThan(0);

    const textBlocks = assistantMsgs.flatMap((m) =>
      m.role === "assistant"
        ? m.content.filter((c) => c.type === "text")
        : [],
    );
    expect(textBlocks.some((t) => t.type === "text" && t.text.includes("Hello from piko!"))).toBe(true);
  });

  it("full pipeline with tools", async () => {
    faux.setResponses([
      fauxAssistantMessage([
        fauxToolCall("search", { query: "piko" }, { id: "call_search" }),
      ]),
      fauxAssistantMessage("Found results about piko."),
    ]);

    const toolRegistry: NativeToolRegistry = {
      search: async (args) => {
        return { results: [`Result for ${args.query}`] };
      },
    };

    const engine = createNativeEngine({ tools: toolRegistry });

    const tools = [{
      name: "search",
      description: "Search tool",
      inputSchema: {
        type: "object",
        properties: { query: { type: "string" } },
      },
      executor: { kind: "native" as const, target: "search" },
    }];

    const config = createHostConfig(
      buildTestModel(),
      buildProviderConfig(),
      createDefaultSettings({ maxSteps: 3 }),
    );

    const host = new PikoHost({ engine, config, tools });

    const result = await host.run("Search for piko");

    expect(result.status).toBe("completed");
    const toolMsgs = result.messages.filter((m) => m.role === "toolResult");
    expect(toolMsgs.length).toBeGreaterThan(0);
  });
});
