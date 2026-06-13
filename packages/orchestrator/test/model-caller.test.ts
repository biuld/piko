import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, registerFauxProvider } from "@earendil-works/pi-ai";
import type { Message } from "piko-orchestrator-protocol";
import { createModelCaller } from "../src/model/model-caller.js";

const PROVIDER = "faux-orchestrator";
const API = "openai-completions";
const MODEL_ID = "faux-orchestrator-model";

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

function buildTestModel(): Model<string> {
  return {
    id: MODEL_ID,
    name: "Faux Orchestrator Model",
    api: API,
    provider: PROVIDER,
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

describe("ModelCaller", () => {
  it("returns assistant message from model call", async () => {
    faux.setResponses([fauxAssistantMessage("Hello from the model")]);

    const executor = createModelCaller();
    const transcript: Message[] = [{ role: "user", content: "Say hello", timestamp: Date.now() }];

    const stream = executor.executeStep({
      runId: "run_test",
      stepId: "step_1",
      transcript,
      systemPrompt: "You are a helpful assistant.",
      model: buildTestModel(),
      provider: {},
      settings: { maxSteps: 5, allowToolCalls: true },
    });

    for await (const _event of stream) {
      // drain stream
    }

    const result = await stream.result();
    expect(result.status).toBe("completed");
    expect(result.appendedMessages.length).toBeGreaterThan(0);
    expect(
      result.appendedMessages.some(
        (msg) =>
          msg.role === "assistant" &&
          Array.isArray(msg.content) &&
          msg.content.some((part) => part.type === "text" && part.text === "Hello from the model"),
      ),
    ).toBe(true);
  });
});
