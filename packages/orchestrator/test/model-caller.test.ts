import { afterAll, beforeAll, describe, expect, it, mock } from "bun:test";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import type { Message } from "piko-orchestrator-protocol";

const PROVIDER = "faux-orchestrator";
const API = "openai-completions";
const MODEL_ID = "faux-orchestrator-model";

let faux: FauxProviderRegistration;
let createModelCaller: any;
let originalStream: any;
let mockPiStream: any = null;

beforeAll(async () => {
  // Statically/dynamically get the real original module's stream and other functions
  const realPiAi = await import("@earendil-works/pi-ai");
  originalStream = realPiAi.stream;

  faux = realPiAi.registerFauxProvider({
    api: API,
    provider: PROVIDER,
    models: [{ id: MODEL_ID }],
  });

  // Register the mock module
  mock.module("@earendil-works/pi-ai", () => {
    return {
      stream: (...args: any[]) => {
        if (mockPiStream) {
          return mockPiStream(...args);
        }
        return originalStream(...args);
      },
    };
  });

  // Dynamically import model-caller to ensure it resolves the mocked stream
  const callerModule = await import("../src/model/model-caller.js");
  createModelCaller = callerModule.createModelCaller;
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

function rejectAfter(ms: number, msg: string): Promise<never> {
  return new Promise((_, reject) => setTimeout(() => reject(new Error(msg)), ms));
}

describe("ModelCaller", () => {
  it("returns assistant message from model call", async () => {
    faux.setResponses([realPiAiMessage("Hello from the model")]);

    const executor = createModelCaller();
    const transcript: Message[] = [{ role: "user", content: "Say hello", timestamp: Date.now() }];

    const stream = executor.executeStep({
      runId: "run_test",
      stepId: "step_1",
      transcript,
      systemPrompt: "You are a helpful assistant.",
      model: buildTestModel(),
      provider: {},
      settings: { allowToolCalls: true },
    });

    for await (const event of stream) {
      if (event.type === "error") {
        console.log("STREAM ERROR IN TEST:", event.message);
      }
    }

    const result = await stream.result();
    expect(result.status).toBe("completed");
    expect(result.appendedMessages.length).toBeGreaterThan(0);
    expect(
      result.appendedMessages.some(
        (msg: any) =>
          msg.role === "assistant" &&
          Array.isArray(msg.content) &&
          msg.content.some(
            (part: any) => part.type === "text" && part.text === "Hello from the model",
          ),
      ),
    ).toBe(true);
  });

  // --- Regression Tests ---

  it("Provider iterator never ends and ignores AbortSignal", async () => {
    mockPiStream = async function* (_model: any, _input: any, _opts: any) {
      yield { type: "start", partial: { content: [] } };
      yield {
        type: "text_start",
        contentIndex: 0,
        partial: { content: [{ type: "text", text: "" }] },
      };
      yield {
        type: "text_delta",
        contentIndex: 0,
        delta: "Hello",
        partial: { content: [{ type: "text", text: "Hello" }] },
      };
      while (true) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        yield { type: "heartbeat" };
      }
    };

    const executor = createModelCaller();
    const abortController = new AbortController();
    const transcript: Message[] = [{ role: "user", content: "Say hello", timestamp: Date.now() }];

    const stream = executor.executeStep(
      {
        runId: "run_test_never_ends",
        stepId: "step_never_ends",
        transcript,
        systemPrompt: "You are a helpful assistant.",
        model: buildTestModel(),
        provider: {},
        settings: { allowToolCalls: true },
      },
      abortController.signal,
    );

    const iterator = stream[Symbol.asyncIterator]();
    const first = await iterator.next();
    expect(first.done).toBe(false);

    abortController.abort();

    const operation = (async () => {
      const second = await iterator.next();
      expect(second.done).toBe(true);
      const result = await stream.result();
      expect(result.status).toBe("aborted");
      expect(result.stopReason).toBe("abort");
      expect(result.appendedMessages).toEqual([]);
    })();

    await Promise.race([operation, rejectAfter(1000, "test hung")]);

    mockPiStream = null;
  });

  it("Idle timeout - returns error when no events occur", async () => {
    mockPiStream = async function* () {
      while (true) {
        await new Promise((resolve) => setTimeout(resolve, 50));
        yield { type: "heartbeat" };
      }
    };

    const executor = createModelCaller({ streamIdleTimeoutMs: 20 });
    const transcript: Message[] = [{ role: "user", content: "Say hello", timestamp: Date.now() }];

    const stream = executor.executeStep({
      runId: "run_test_timeout",
      stepId: "step_timeout",
      transcript,
      systemPrompt: "You are a helpful assistant.",
      model: buildTestModel(),
      provider: {},
      settings: { allowToolCalls: true },
    });

    const operation = (async () => {
      const events: any[] = [];
      for await (const event of stream) {
        events.push(event);
      }
      expect(events.length).toBeGreaterThan(0); // Should yield step_start

      const result = await stream.result();
      expect(result.status).toBe("error");
      expect(result.stopReason).toBe("error");
    })();

    await Promise.race([operation, rejectAfter(1000, "test hung")]);

    mockPiStream = null;
  });

  it("Idle timeout - timer resets when events continue to arrive", async () => {
    mockPiStream = async function* () {
      for (let i = 0; i < 3; i++) {
        await new Promise((resolve) => setTimeout(resolve, 10));
        yield { type: "text_delta", contentIndex: 0, delta: `${i}`, partial: { content: [] } };
      }
      yield {
        type: "done",
        message: { role: "assistant", content: [{ type: "text", text: "Done" }] },
      };
    };

    const executor = createModelCaller({ streamIdleTimeoutMs: 20 });
    const transcript: Message[] = [{ role: "user", content: "Say hello", timestamp: Date.now() }];

    const stream = executor.executeStep({
      runId: "run_test_reset",
      stepId: "step_reset",
      transcript,
      systemPrompt: "You are a helpful assistant.",
      model: buildTestModel(),
      provider: {},
      settings: { allowToolCalls: true },
    });

    const operation = (async () => {
      const events: any[] = [];
      for await (const event of stream) {
        events.push(event);
      }
      expect(events.length).toBeGreaterThan(0);
      const result = await stream.result();
      expect(result.status).toBe("completed");
    })();

    await Promise.race([operation, rejectAfter(1000, "test hung")]);

    mockPiStream = null;
  });
});

// Helper wrapper for fauxAssistantMessage since we import pi-ai dynamically
function realPiAiMessage(text: string) {
  return {
    role: "assistant" as const,
    content: [{ type: "text" as const, text }],
    api: "openai-completions" as const,
    provider: "unknown",
    model: "unknown",
    usage: {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      totalTokens: 0,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
    },
    stopReason: "stop" as const,
    timestamp: Date.now(),
  };
}
