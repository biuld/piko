import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, registerFauxProvider } from "@earendil-works/pi-ai";
import { createNativeEngine } from "piko-engine-native";

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createDefaultSettings, createHostConfig, PikoHost } from "../src/index.js";

// ============================================================================
// TUI Consistency tests (Phase 6)
// ============================================================================

describe("TUI Consistency", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-tui-consistency",
      models: [{ id: "model-a" }, { id: "model-b" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function tuiModel(id: string): Model<string> {
    return {
      id,
      name: `Model ${id}`,
      api: "openai-completions",
      provider: "faux-tui-consistency",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  it("should default host settings to parallel tool execution", () => {
    expect(createDefaultSettings().parallelTools).toBe(true);
    expect(createHostConfig(tuiModel("model-a")).settings.parallelTools).toBe(true);
  });

  it("should keep getConfig in sync after setConfig", async () => {
    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(tuiModel("model-a")),
    });

    expect(host.getConfig().model.id).toBe("model-a");

    host.setConfig(createHostConfig(tuiModel("model-b")));

    expect(host.getConfig().model.id).toBe("model-b");
  });

  it("should keep getThinkingLevel in sync after setThinkingLevel", async () => {
    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(tuiModel("model-a")),
    });

    expect(host.getThinkingLevel()).toBe("off");

    host.setThinkingLevel("high");
    expect(host.getThinkingLevel()).toBe("high");

    host.setThinkingLevel("medium");
    expect(host.getThinkingLevel()).toBe("medium");
  });

  it("should return to idle phase after run completes", async () => {
    faux.setResponses([fauxAssistantMessage("Done")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(tuiModel("model-a"), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    expect(() => host.nextTurn("test")).not.toThrow();

    const result = await host.run("Test");
    expect(result.status).toBe("completed");

    expect(() => host.steer("test")).toThrow("Cannot steer while idle");
    expect(() => host.followUp("test")).toThrow("Cannot follow up while idle");
    expect(() => host.nextTurn("test")).not.toThrow();
  });

  it("should return to idle phase after stream completes", async () => {
    faux.setResponses([fauxAssistantMessage("Streamed")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(tuiModel("model-a"), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    const stream = host.streamPrompt("Stream");
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    expect(() => host.steer("test")).toThrow("Cannot steer while idle");
  });

  it("should set and retrieve session name", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-tui-name-"));

    faux.setResponses([fauxAssistantMessage("Named")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(tuiModel("model-a"), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
      session: { cwd },
    });

    await host.run("Initial");
    await host.setSessionName("My Named Session");

    const name = await host.getSessionName();
    expect(name).toBe("My Named Session");
  });
});
