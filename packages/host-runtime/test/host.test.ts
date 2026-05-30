import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { registerFauxProvider, fauxAssistantMessage, fauxToolCall } from "@earendil-works/pi-ai";
import type { FauxProviderRegistration } from "@earendil-works/pi-ai";
import * as fs from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createNativeEngine } from "piko-engine-native";
import type { NativeToolRegistry } from "piko-engine-native";
import type { EngineModel } from "piko-engine-protocol";
import { PikoHost, createHostConfig, createPiLlmCaller, SessionManager } from "../src/index.js";

const PROVIDER = "faux";
const API = "openai-completions";
const MODEL_ID = "faux-host-model";

let faux: FauxProviderRegistration;
const originalHome = process.env.HOME;

beforeAll(() => {
  faux = registerFauxProvider({
    api: API,
    provider: PROVIDER,
    models: [{ id: MODEL_ID }],
  });
});

beforeEach(async () => {
  process.env.HOME = await fs.mkdtemp(join(tmpdir(), "piko-host-test-home-"));
});

afterAll(() => {
  faux?.unregister();
  process.env.HOME = originalHome;
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

  it("should persist and resume transcript through SessionManager", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-cwd-"));

    faux.setResponses([
      fauxAssistantMessage("First reply"),
      fauxAssistantMessage("Second reply"),
    ]);

    const sessionManager = await SessionManager.create(cwd);
    const engine = createNativeEngine({ llmCaller: createPiLlmCaller() });
    const config = createHostConfig(buildTestModel(), undefined, { allowToolCalls: false, maxSteps: 1 });

    const host = new PikoHost({ engine, config, sessionManager, cwd });
    const first = await host.run("First prompt");
    expect(first.messages).toHaveLength(2);
    expect(first.sessionFile).toBeDefined();

    const reopened = await SessionManager.open(first.sessionId, cwd);
    expect(reopened).not.toBeNull();

    const resumedHost = new PikoHost({
      engine: createNativeEngine({ llmCaller: createPiLlmCaller() }),
      config,
      sessionManager: reopened!,
      cwd,
    });
    const second = await resumedHost.run("Second prompt");

    expect(second.messages.filter((message) => message.role === "user")).toHaveLength(2);
    expect(second.messages.filter((message) => message.role === "assistant")).toHaveLength(2);
  });

  it("should stream a prompt through host runtime and persist the result", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-stream-cwd-"));

    faux.setResponses([fauxAssistantMessage("Streaming reply")]);

    const sessionManager = await SessionManager.create(cwd);
    const engine = createNativeEngine({ llmCaller: createPiLlmCaller() });
    const config = createHostConfig(buildTestModel(), undefined, {
      allowToolCalls: false,
      maxSteps: 1,
      stopConditions: { stopOnAssistantMessage: true },
    });
    const host = new PikoHost({ engine, config, sessionManager, cwd });

    const stream = host.streamPrompt("Stream this");
    const events = [];
    for await (const event of stream) {
      events.push(event.type);
    }
    const result = await stream.result();

    expect(events).toContain("message_delta");
    expect(result.status).toBe("completed");
    expect(result.messages.filter((message) => message.role === "user")).toHaveLength(1);
    expect(result.messages.filter((message) => message.role === "assistant")).toHaveLength(1);

    const resumed = await SessionManager.open(result.sessionId, cwd);
    const persisted = await resumed?.loadMessages();
    expect(persisted).toHaveLength(2);
  });
});
