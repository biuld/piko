import type { EngineInput } from "piko-engine-protocol";
import { describe, expect, it } from "vitest";
import { createNativeEngine } from "../src/engine.js";
import { buildAssistantMessage } from "../src/transcript-builder.js";
import { emptyUsage, makeFauxAdapter, makeModel, makeSettings } from "./helpers.js";

describe("Engine Continuation State", () => {
  it("should preserve counters across steps", async () => {
    const engine = createNativeEngine({
      providerAdapter: makeFauxAdapter(() => ({
        messages: [buildAssistantMessage("all done", [])],
        usage: emptyUsage,
      })),
    });

    const input: EngineInput = {
      runId: "test-run",
      stepId: "step-1",
      transcript: [],
      systemPrompt: "test",
      model: makeModel(),
      provider: {},
      settings: makeSettings({
        allowToolCalls: true,
        stopConditions: { stopOnAssistantMessage: true },
      }),
    };

    const result = await engine.executeStep(input).result();
    expect(result.engineState).toBeDefined();

    const cs = result.engineState as {
      version: number;
      counters?: { modelCalls: number };
    };
    expect(cs.version).toBe(1);
    expect(cs.counters?.modelCalls).toBeGreaterThanOrEqual(1);
  });
});
