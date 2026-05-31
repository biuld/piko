import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, registerFauxProvider } from "@earendil-works/pi-ai";
import { createNativeEngine } from "piko-engine-native";

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { HostLifecycleEvent } from "../src/index.js";
import { createHostConfig, PikoHost } from "../src/index.js";

// ============================================================================
// Queue Semantics tests (Phase 3)
// ============================================================================

describe("Queue Semantics", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-queue",
      models: [{ id: "faux-queue-model" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function qModel(): Model<string> {
    return {
      id: "faux-queue-model",
      name: "Faux Queue Model",
      api: "openai-completions",
      provider: "faux-queue",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  it("should reject steer() when idle", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-queue-idle-"));
    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(qModel()),
      session: { cwd },
    });
    expect(() => host.steer("test")).toThrow("Cannot steer while idle");
  });

  it("should reject followUp() when idle", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-queue-idle-"));
    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(qModel()),
      session: { cwd },
    });
    expect(() => host.followUp("test")).toThrow("Cannot follow up while idle");
  });

  it("should allow nextTurn() when idle", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-queue-idle-"));
    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(qModel()),
      session: { cwd },
    });
    expect(() => host.nextTurn("test")).not.toThrow();
  });

  it("should allow steer() and followUp() during a run", async () => {
    faux.setResponses([fauxAssistantMessage("Running")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(qModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    let steered = false;
    let followedUp = false;
    const stream = host.streamPrompt("Run", {
      onLifecycleEvent: (e) => {
        if (e.type === "agent_start") {
          if (!steered) {
            host.steer("steer");
            steered = true;
          }
          if (!followedUp) {
            host.followUp("follow");
            followedUp = true;
          }
        }
      },
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    expect(() => host.steer("test")).toThrow("Cannot steer while idle");
    expect(() => host.followUp("test")).toThrow("Cannot follow up while idle");
  });

  it("should consume steering in all mode by default", async () => {
    faux.setResponses([fauxAssistantMessage("Final")]);

    const host = await PikoHost.create({
      engine: createNativeEngine(),
      config: createHostConfig(qModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
    });

    let queued = false;
    const lifecycle: HostLifecycleEvent[] = [];
    const stream = host.streamPrompt("Run", {
      onLifecycleEvent: (e) => {
        lifecycle.push(e);
        if (e.type === "agent_start" && !queued) {
          queued = true;
          host.steer("Steer A");
          host.steer("Steer B");
        }
      },
    });
    for await (const _event of stream) {
      /* consume */
    }
    await stream.result();

    const queueUpdates = lifecycle.filter((e) => e.type === "queue_update");
    expect(queueUpdates.length).toBeGreaterThanOrEqual(1);

    const lastUpdate = queueUpdates[queueUpdates.length - 1];
    if (lastUpdate?.type === "queue_update") {
      expect(lastUpdate.steerCount).toBe(0);
    }
  });
});
