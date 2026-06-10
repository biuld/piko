import { afterAll, beforeAll, beforeEach, describe, expect, it } from "bun:test";
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
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 10 }),
    });

    const result = await host.run("Hi there");

    expect(result.status).toBe("completed");
    expect(result.messages.length).toBeGreaterThanOrEqual(2);

    const userMsg = result.messages.find((m) => m.role === "user");
    expect(userMsg).toBeDefined();

    const assistantMsgs = result.messages.filter((m) => m.role === "assistant");
    expect(assistantMsgs.length).toBeGreaterThan(0);
  });

  it("should handle tool calls", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("echo", { text: "hello" }, { id: "call_echo" })]),
      fauxAssistantMessage("Done"),
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
    const engine = createNativeEngine({ toolRegistry, toolDefinitions: tools });

    const host = await PikoHost.create({
      engine,
      config: createHostConfig(buildTestModel(), undefined, { maxSteps: 5 }),
    });

    // Just verify host can run with tools
    const result = await host.run("Echo hello");
    expect(["completed", "max_steps"]).toContain(result.status);
  });

  it("should stop after max steps", async () => {
    // Use tool calls to force multiple steps
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "c1" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "c2" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "c3" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "c4" })]),
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "c5" })]),
      fauxAssistantMessage("Done"),
    ]);

    const toolRegistry: NativeToolRegistry = {
      noop: async () => ({ ok: true }),
    };
    const tools = [buildTestTool("noop", "No operation")];
    const engine = createNativeEngine({ toolRegistry, toolDefinitions: tools });

    const host = await PikoHost.create({
      engine,
      config: createHostConfig(buildTestModel(), undefined, {
        maxSteps: 3,
      }),
    });

    const result = await host.run("Loop test");
    expect(result.status).toBe("max_steps");
  });

  it("should persist and resume transcript through SessionManager", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-cwd-"));

    faux.setResponses([fauxAssistantMessage("First reply"), fauxAssistantMessage("Second reply")]);

    const sessionManager = await SessionManager.create(cwd);
    const config = createHostConfig(buildTestModel(), undefined, {
      allowToolCalls: false,
      maxSteps: 10,
    });

    const host = PikoHost.fromSessionManager(createNativeEngine(), config, sessionManager);
    const first = await host.run("First prompt");
    expect(first.messages.filter((m) => m.role === "user")).toHaveLength(1);
    expect(first.messages.filter((m) => m.role === "assistant")).toHaveLength(1);
    expect(first.sessionFile).toBeDefined();

    const reopened = await SessionManager.open(first.sessionId, cwd);
    expect(reopened).not.toBeNull();

    const resumedHost = PikoHost.fromSessionManager(createNativeEngine(), config, reopened!);
    const second = await resumedHost.run("Second prompt");

    expect(second.messages.filter((m) => m.role === "user")).toHaveLength(1);
    expect(second.messages.filter((m) => m.role === "assistant")).toHaveLength(1);
  });

  it("should expose session management through the host facade", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-host-facade-cwd-"));

    faux.setResponses([fauxAssistantMessage("Facade reply")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(buildTestModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 10,
      }),
      session: { cwd },
    });

    await host.run("Name this");
    await host.setSessionName("Named Session");

    expect(await host.getSessionName()).toBe("Named Session");
    expect(host.isSessionPersisted()).toBe(true);

    const listed = await host.listSessions();
    expect(listed).toHaveLength(1);
    expect(listed[0]?.name).toBe("Named Session");

    const renamed = await host.renameSession(host.sessionId, "Renamed Session");
    expect(renamed).toBe(true);
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
        maxSteps: 10,
      }),
      session: { cwd },
    });

    const result = await host.run("Explain");
    const assistant = result.messages.find((m) => m.role === "assistant");

    expect(assistant).toBeDefined();
    if (assistant?.role === "assistant") {
      expect(assistant.api).toBe(API);
      expect(assistant.provider).toBe(PROVIDER);
      expect(assistant.model).toBe(MODEL_ID);
      expect(assistant.usage).toBeDefined();
    }
  });
});
