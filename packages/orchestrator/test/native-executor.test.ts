import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, fauxToolCall, registerFauxProvider } from "@earendil-works/pi-ai";
import type { Message } from "../src/model/event-stream.js";
import { createNativeModelExecutor } from "../src/model/native-executor.js";
import type { ToolDef } from "../src/tools/types.js";

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

describe("NativeModelExecutor", () => {
  it("preserves model context when resolving tool resources", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("echo", { text: "hello" }, { id: "call_echo" })]),
      fauxAssistantMessage("Done"),
    ]);

    const tools: ToolDef[] = [
      {
        name: "echo",
        description: "Echoes back text",
        inputSchema: { type: "object", properties: { text: { type: "string" } } },
        executor: { kind: "native", target: "echo" },
      },
    ];
    const executor = createNativeModelExecutor({ toolDefinitions: tools });
    const transcript: Message[] = [{ role: "user", content: "Echo hello", timestamp: Date.now() }];

    const stream = executor.executeStep({
      runId: "run_test",
      stepId: "step_1",
      transcript,
      systemPrompt: "You are testing tool resolution.",
      model: buildTestModel(),
      provider: {},
      tools,
      settings: { maxSteps: 5, allowToolCalls: true },
    });

    for await (const _event of stream) {
      // drain stream
    }
    const first = await stream.result();
    expect(first.status).toBe("awaiting_resource");

    const resolved = await executor.resolveResource!({
      runId: "run_test",
      stepId: "step_1_resolve",
      transcript: [...transcript, ...first.appendedMessages],
      engineState: first.engineState,
      toolResults: [{ toolCallId: "call_echo", result: { text: "hello" }, isError: false }],
    });

    expect(resolved.status).toBe("completed");
    expect(
      resolved.appendedMessages.some(
        (msg) =>
          msg.role === "assistant" &&
          Array.isArray(msg.content) &&
          msg.content.some((part) => part.type === "text" && part.text === "Done"),
      ),
    ).toBe(true);
  });
});
