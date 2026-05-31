import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, fauxToolCall, registerFauxProvider } from "@earendil-works/pi-ai";
import type { NativeToolRegistry } from "piko-engine-native";
import { createNativeEngine } from "piko-engine-native";

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createHostConfig, PikoHost } from "../src/index.js";

// ============================================================================
// Session Write / Save Point tests (Phase 4)
// ============================================================================

describe("Session Write / Save Points", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-savepoint",
      models: [{ id: "faux-savepoint-model" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function spModel(): Model<string> {
    return {
      id: "faux-savepoint-model",
      name: "Faux SavePoint Model",
      api: "openai-completions",
      provider: "faux-savepoint",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  it("should persist messages incrementally at save points (turn boundaries)", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-savepoint-cwd-"));

    faux.setResponses([
      fauxAssistantMessage([fauxToolCall("noop", {}, { id: "call_sp_1" })]),
      fauxAssistantMessage("Final reply after tool"),
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
      config: createHostConfig(spModel(), undefined, {
        allowToolCalls: true,
        maxSteps: 5,
      }),
      session: { cwd },
    });

    const savePoints: number[] = [];
    const stream = host.streamPrompt("Multi-step", {
      onLifecycleEvent: (e) => {
        if (e.type === "save_point") {
          savePoints.push(savePoints.length);
        }
      },
    });
    for await (const _event of stream) {
      /* consume */
    }
    const result = await stream.result();

    expect(result.status).toBe("completed");
    expect(savePoints.length).toBeGreaterThanOrEqual(2);

    const persisted = await host.loadMessages();
    expect(persisted.length).toBeGreaterThanOrEqual(4);
  });

  it("should flush saves on abort", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-savepoint-abort-"));

    faux.setResponses([fauxAssistantMessage("Slow response")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(spModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
      session: { cwd },
    });

    const controller = new AbortController();
    controller.abort();

    try {
      const stream = host.streamPrompt("Abort", {}, controller.signal);
      for await (const _event of stream) {
        /* consume */
      }
    } catch {
      // Expected
    }

    const persisted = await host.loadMessages();
    expect(persisted.length).toBeGreaterThanOrEqual(1);
    expect(persisted.some((m) => m.role === "user")).toBe(true);
  });

  it("should reject session mutations during a run", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-phase-mutation-"));

    faux.setResponses([fauxAssistantMessage("Running")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(spModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
      session: { cwd },
    });

    let mutationRejected = false;
    const stream = host.streamPrompt("Run", {
      onLifecycleEvent: async (e) => {
        if (e.type === "agent_start") {
          try {
            await host.branchToEntry("some-id");
          } catch {
            mutationRejected = true;
          }
        }
      },
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    expect(mutationRejected).toBe(true);
  });
});
