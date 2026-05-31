import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FauxProviderRegistration } from "@earendil-works/pi-ai";
import {
  fauxAssistantMessage,
  fauxThinking,
  fauxToolCall,
  registerFauxProvider,
} from "@earendil-works/pi-ai";
import type { NativeToolRegistry } from "piko-engine-native";
import { createNativeEngine } from "piko-engine-native";

import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { createHostConfig, PikoHost, SessionManager } from "../src/index.js";

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

function buildTestModel(): Model<string> {
  return {
    id: MODEL_ID,
    name: "Faux Host Model",
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

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(buildTestModel()),
    });

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
      fauxAssistantMessage([fauxToolCall("echo", { text: "hello" }, { id: "call_echo" })]),
    ]);

    const toolRegistry: NativeToolRegistry = {
      echo: async (args) => {
        return { echoed: args.text };
      },
    };

    const tools = [
      buildTestTool("echo", "Echoes back the text", {
        type: "object",
        properties: { text: { type: "string" } },
      }),
    ];

    const engine = createNativeEngine({ toolRegistry, toolDefinitions: tools });

    const host = await PikoHost.create({
      engine,
      config: createHostConfig(buildTestModel()),
    });

    const result = await host.run("Echo hello");

    expect(result.status).toBe("completed");
    expect(result.totalSteps).toBeGreaterThanOrEqual(1);

    const toolMsgs = result.messages.filter((m) => m.role === "toolResult");
    expect(toolMsgs.length).toBeGreaterThan(0);

    if (toolMsgs.length > 0 && toolMsgs[0].role === "toolResult") {
      expect(toolMsgs[0].toolName).toBe("echo");
      expect(toolMsgs[0].isError).toBe(false);
      expect(toolMsgs[0].details).toEqual({ echoed: "hello" });
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

    const tools = [buildTestTool("noop", "No operation")];
    const engine = createNativeEngine({ toolRegistry, toolDefinitions: tools });

    const host = await PikoHost.create({
      engine,
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 3 }),
    });

    const result = await host.run("Loop test");

    expect(result.status).toBe("max_steps");
    expect(result.totalSteps).toBe(3);
  });

  it("should persist and resume transcript through SessionManager", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-cwd-"));

    faux.setResponses([fauxAssistantMessage("First reply"), fauxAssistantMessage("Second reply")]);

    const sessionManager = await SessionManager.create(cwd);
    const config = createHostConfig(buildTestModel(), undefined, {
      allowToolCalls: false,
      maxSteps: 1,
    });

    const host = PikoHost.fromSessionManager(createNativeEngine(), config, sessionManager);
    const first = await host.run("First prompt");
    expect(first.messages).toHaveLength(2);
    expect(first.sessionFile).toBeDefined();

    const reopened = await SessionManager.open(first.sessionId, cwd);
    expect(reopened).not.toBeNull();

    const resumedHost = PikoHost.fromSessionManager(createNativeEngine(), config, reopened!);
    const second = await resumedHost.run("Second prompt");

    expect(second.messages.filter((message) => message.role === "user")).toHaveLength(2);
    expect(second.messages.filter((message) => message.role === "assistant")).toHaveLength(2);
  });

  it("should expose session management through the host facade", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-facade-cwd-"));

    faux.setResponses([fauxAssistantMessage("Facade reply")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(buildTestModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 1,
      }),
      session: { cwd },
    });

    await host.run("Name this");
    await host.setSessionName("Named Session");

    expect(await host.getSessionName()).toBe("Named Session");
    expect(host.isSessionPersisted()).toBe(true);
    expect(await host.loadMessages()).toHaveLength(2);

    const listed = await host.listSessions();
    expect(listed).toHaveLength(1);
    expect(listed[0]?.name).toBe("Named Session");

    const renamed = await host.renameSession(host.sessionId, "Renamed Session");
    expect(renamed).toBe(true);
    // Re-open session to pick up rename (rename opens a separate Session handle)
    await host.switchSession(host.sessionId);
    expect(await host.getSessionName()).toBe("Renamed Session");

    const branchEntries = await host.getBranchEntries();
    const userEntry = branchEntries.find(
      (entry) =>
        entry.type === "message" &&
        entry.message.role === "user" &&
        entry.message.content === "Name this",
    );
    expect(userEntry).toBeDefined();

    await host.branchToEntry(userEntry!.id);
    expect(host.getLeafId()).toBe(userEntry!.id);

    // Delete the original session (listed before newSession was called)
    const deleted = await host.deleteSession(listed[0]!.id);
    expect(deleted).toBe(true);
  });

  it("should persist pi-style assistant metadata and thinking blocks", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-metadata-cwd-"));

    faux.setResponses([
      fauxAssistantMessage(
        [fauxThinking("Reason privately"), { type: "text", text: "Final answer" }],
        {
          responseId: "resp_123",
          stopReason: "toolUse",
        },
      ),
    ]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(buildTestModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 1,
      }),
      session: { cwd },
    });

    const result = await host.run("Explain");
    const assistant = result.messages.find((message) => message.role === "assistant");

    expect(assistant).toBeDefined();
    if (assistant?.role === "assistant") {
      expect(assistant.api).toBe(API);
      expect(assistant.provider).toBe(PROVIDER);
      expect(assistant.model).toBe(MODEL_ID);
      expect(assistant.responseId).toBe("resp_123");
      expect(assistant.stopReason).toBe("toolUse");
      expect(assistant.content).toEqual([
        { type: "thinking", thinking: "Reason privately" },
        { type: "text", text: "Final answer" },
      ]);
      expect(assistant.usage).toMatchObject({
        input: expect.any(Number),
        output: expect.any(Number),
        cacheRead: 0,
        cacheWrite: 0,
        totalTokens: expect.any(Number),
        cost: {
          input: 0,
          output: 0,
          cacheRead: 0,
          cacheWrite: 0,
          total: 0,
        },
      });
    }

    const persisted = await host.loadMessages();
    const persistedAssistant = persisted.find((message) => message.role === "assistant");
    expect(persistedAssistant).toMatchObject({
      role: "assistant",
      api: API,
      provider: PROVIDER,
      model: MODEL_ID,
      responseId: "resp_123",
      stopReason: "toolUse",
    });
  });

  it("should stream a prompt through host runtime and persist the result", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-stream-cwd-"));

    faux.setResponses([fauxAssistantMessage("Streaming reply")]);

    const sessionManager = await SessionManager.create(cwd);
    const config = createHostConfig(buildTestModel(), undefined, {
      allowToolCalls: false,
      maxSteps: 1,
      stopConditions: { stopOnAssistantMessage: true },
    });
    const host = PikoHost.fromSessionManager(createNativeEngine(), config, sessionManager);

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

  it("should stream multi-step tool execution events through host runtime", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-stream-tool-cwd-"));

    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("echo", { text: "hello" }, { id: "call_echo_stream" })]),
      fauxAssistantMessage("Tool loop done"),
    ]);

    const toolRegistry: NativeToolRegistry = {
      echo: async (args) => ({ echoed: args.text }),
    };
    const tools = [
      buildTestTool("echo", "Echoes back the text", {
        type: "object",
        properties: { text: { type: "string" } },
      }),
    ];

    const host = await PikoHost.create({
      engine: createNativeEngine({ toolRegistry, toolDefinitions: tools }),
      config: createHostConfig(buildTestModel(), undefined, {
        allowToolCalls: true,
        maxSteps: 4,
      }),
      session: { cwd },
    });

    const stream = host.streamPrompt("Stream tool loop");
    const eventTypes: string[] = [];
    for await (const event of stream) {
      eventTypes.push(event.type);
    }
    const result = await stream.result();

    expect(eventTypes).toContain("tool_call_start");
    expect(eventTypes).toContain("tool_call_end");
    expect(eventTypes).toContain("message_delta");
    expect(result.status).toBe("completed");
    expect(result.messages.filter((message) => message.role === "toolResult")).toHaveLength(1);
    expect(result.messages.filter((message) => message.role === "assistant")).toHaveLength(2);

    const persisted = await host.loadMessages();
    expect(persisted.filter((message) => message.role === "toolResult")).toHaveLength(1);
  });
});
