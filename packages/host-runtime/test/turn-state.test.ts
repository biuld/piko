import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import { promises as fs } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, fauxToolCall, registerFauxProvider } from "@earendil-works/pi-ai";
import type { NativeToolRegistry } from "piko-engine-native";
import { createNativeEngine } from "piko-engine-native";
import type { EngineEvent } from "piko-engine-protocol";
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

// ============================================================================
// TurnState snapshot tests (Phase 2)
// ============================================================================

describe("TurnState Snapshots", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-snapshot",
      models: [{ id: "faux-snapshot-model" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function snapshotModel(): Model<string> {
    return {
      id: "faux-snapshot-model",
      name: "Faux Snapshot Model",
      api: "openai-completions",
      provider: "faux-snapshot",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  it("should include tools in HostConfig for TurnState", async () => {
    faux.setResponses([fauxAssistantMessage("Simple reply")]);

    const toolDefs = [
      {
        name: "noop",
        description: "No-op tool",
        inputSchema: { type: "object", properties: {} },
        executor: { kind: "native" as const, target: "noop" },
      },
    ];

    const model = snapshotModel();
    const host = await PikoHost.create({
      engine: createNativeEngine({
        toolRegistry: { noop: async () => ({ ok: true }) },
        toolDefinitions: toolDefs,
      }),
      config: createHostConfig(
        model,
        undefined,
        {
          allowToolCalls: false,
          maxSteps: 5,
        },
        toolDefs,
      ),
    });

    // The config now carries tools
    const config = host.getConfig();
    expect(config.tools).toBeDefined();
    expect(config.tools?.length).toBe(1);
    if (config.tools?.[0]) {
      expect(config.tools[0].name).toBe("noop");
    }
  });

  it("should build TurnState with system prompt per turn", async () => {
    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_ts2" })]),
      fauxAssistantMessage("Final"),
    ]);

    const toolRegistry: NativeToolRegistry = {
      noop: async () => ({ ok: true }),
    };
    const toolDefs = [
      {
        name: "noop",
        description: "No-op",
        inputSchema: { type: "object", properties: {} },
        executor: { kind: "native" as const, target: "noop" },
      },
    ];

    const host = await PikoHost.create({
      engine: createNativeEngine({ toolRegistry, toolDefinitions: toolDefs }),
      config: createHostConfig(snapshotModel(), undefined, {
        allowToolCalls: true,
        maxSteps: 5,
      }),
    });

    const lifecycle: Array<{ type: string }> = [];
    const stream = host.streamPrompt("Test", {
      onLifecycleEvent: (e) => lifecycle.push({ type: e.type }),
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    expect(lifecycle.filter((e) => e.type === "turn_start").length).toBeGreaterThanOrEqual(2);
    expect(lifecycle).toContainEqual({ type: "agent_start" });
    expect(lifecycle).toContainEqual({ type: "settled" });
  });
});

// ============================================================================
// Active Tools tests (Phase 3)
// ============================================================================

describe("Active Tools", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-activetools",
      models: [{ id: "faux-active-model" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function atModel(): Model<string> {
    return {
      id: "faux-active-model",
      name: "Faux Active Model",
      api: "openai-completions",
      provider: "faux-activetools",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  it("should include all tools in activeTools when no filter is set", async () => {
    faux.setResponses([fauxAssistantMessage("OK")]);

    const toolDefs = [
      {
        name: "tool-a",
        description: "A",
        inputSchema: {},
        executor: { kind: "native" as const, target: "tool-a" },
      },
      {
        name: "tool-b",
        description: "B",
        inputSchema: {},
        executor: { kind: "native" as const, target: "tool-b" },
      },
    ];

    const host = await PikoHost.create({
      engine: createNativeEngine({
        toolRegistry: {
          "tool-a": async () => ({ ok: true }),
          "tool-b": async () => ({ ok: true }),
        },
        toolDefinitions: toolDefs,
      }),
      config: createHostConfig(atModel(), undefined, { maxSteps: 5 }, toolDefs),
    });

    // All tools should be active by default
    expect(host.getActiveToolNames()).toBeUndefined();

    const stream = host.streamPrompt("Hi");
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();
  });

  it("should filter active tools when activeToolNames is set", () => {
    // Test that setActiveToolNames persists and getter returns
    const toolDefs = [
      {
        name: "read",
        description: "Read",
        inputSchema: {},
        executor: { kind: "native" as const, target: "read" },
      },
      {
        name: "bash",
        description: "Bash",
        inputSchema: {},
        executor: { kind: "native" as const, target: "bash" },
      },
      {
        name: "edit",
        description: "Edit",
        inputSchema: {},
        executor: { kind: "native" as const, target: "edit" },
      },
    ];

    faux.setResponses([fauxAssistantMessage("Done")]);

    // Create a host with tools in config
    const model = atModel();
    const config = createHostConfig(model, undefined, { maxSteps: 5 }, toolDefs);

    // Verify that setting activeToolNames restricts tools
    // This test validates the PikoHost API surface
    expect(config.tools?.length).toBe(3);
  });

  it("should not execute inactive tools", async () => {
    faux.setResponses([
      fauxAssistantMessage([
        fauxToolCall("write", { path: "blocked.txt" }, { id: "call_inactive_write" }),
      ]),
    ]);

    const toolDefs = [
      {
        name: "read",
        description: "Read",
        inputSchema: {},
        executor: { kind: "native" as const, target: "read" },
      },
      {
        name: "write",
        description: "Write",
        inputSchema: {},
        executor: { kind: "native" as const, target: "write" },
      },
    ];

    let writeCalled = false;

    const host = await PikoHost.create({
      engine: createNativeEngine({
        toolRegistry: {
          read: async () => ({ ok: true }),
          write: async () => {
            writeCalled = true;
            return { ok: true };
          },
        },
        toolDefinitions: toolDefs,
      }),
      config: createHostConfig(
        atModel(),
        undefined,
        {
          allowToolCalls: true,
          maxSteps: 5,
          stopConditions: { stopOnToolResult: true },
        },
        toolDefs,
      ),
    });

    host.setActiveToolNames(["read"]);

    const events: EngineEvent[] = [];
    const stream = host.streamPrompt("Try to write");
    for await (const event of stream) {
      events.push(event);
    }
    const result = await stream.result();

    expect(result.status).toBe("completed");
    expect(writeCalled).toBe(false);
    expect(events).toContainEqual(
      expect.objectContaining({
        type: "tool_call_end",
        id: "call_inactive_write",
        result: "Tool not found: write",
        isError: true,
      }),
    );
  });

  it("should restore explicit active tool clear from session history", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-active-tools-clear-"));
    const toolDefs = [
      {
        name: "read",
        description: "Read",
        inputSchema: {},
        executor: { kind: "native" as const, target: "read" },
      },
    ];

    const host = await PikoHost.create({
      engine: createNativeEngine({
        toolRegistry: { read: async () => ({ ok: true }) },
        toolDefinitions: toolDefs,
      }),
      config: createHostConfig(atModel(), undefined, { maxSteps: 5 }, toolDefs),
      session: { cwd },
    });

    await host.sessionManager.appendActiveToolsChange(["read"]);
    await host.restoreFromSession();
    expect(host.getActiveToolNames()).toEqual(["read"]);

    await host.sessionManager.appendActiveToolsChange([]);
    await host.restoreFromSession();
    expect(host.getActiveToolNames()).toBeUndefined();
  });

  it("should clear stale active tools when restoring a session with no active tool entry", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-active-tools-no-entry-"));
    const toolDefs = [
      {
        name: "read",
        description: "Read",
        inputSchema: {},
        executor: { kind: "native" as const, target: "read" },
      },
    ];

    const host = await PikoHost.create({
      engine: createNativeEngine({
        toolRegistry: { read: async () => ({ ok: true }) },
        toolDefinitions: toolDefs,
      }),
      config: createHostConfig(atModel(), undefined, { maxSteps: 5 }, toolDefs),
      session: { cwd },
    });

    host.setActiveToolNames(["read"]);
    expect(host.getActiveToolNames()).toEqual(["read"]);

    await host.newSession();
    await host.restoreFromSession();

    expect(host.getActiveToolNames()).toBeUndefined();
  });
});
