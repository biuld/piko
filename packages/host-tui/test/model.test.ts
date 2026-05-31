import type { Model } from "@earendil-works/pi-ai";
import type { ResolvedModel } from "piko-host-runtime";
import { describe, expect, it } from "vitest";
import { doApplyModelChange } from "../src/app/model.js";
import { filterModelSelectorEntries } from "../src/overlays/model-selector.js";

function testModel(id: string): Model<string> {
  return {
    id,
    name: `Model ${id}`,
    api: "openai-completions",
    provider: "faux-tui-model",
    baseUrl: "http://localhost:0",
    reasoning: false,
    input: ["text"],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 128000,
    maxTokens: 16384,
  };
}

describe("model helpers", () => {
  it("filters model selector entries by provider, id, full id, or display name", () => {
    const entries = [
      { model: { ...testModel("claude-sonnet"), provider: "anthropic" }, providerConfig: {} },
      {
        model: { ...testModel("gpt-4o"), provider: "openai", name: "GPT 4 Omni" },
        providerConfig: {},
      },
      { model: { ...testModel("gemini-pro"), provider: "google" }, providerConfig: {} },
    ];

    expect(filterModelSelectorEntries(entries, "sonnet").map((e) => e.model.id)).toEqual([
      "claude-sonnet",
    ]);
    expect(filterModelSelectorEntries(entries, "openai/gpt").map((e) => e.model.id)).toEqual([
      "gpt-4o",
    ]);
    expect(filterModelSelectorEntries(entries, "omni").map((e) => e.model.id)).toEqual(["gpt-4o"]);
    expect(filterModelSelectorEntries(entries, "google").map((e) => e.model.id)).toEqual([
      "gemini-pro",
    ]);
  });

  it("returns all model selector entries when the search is empty", () => {
    const entries = [
      { model: testModel("model-a"), providerConfig: {} },
      { model: testModel("model-b"), providerConfig: {} },
    ];

    expect(filterModelSelectorEntries(entries, " ")).toHaveLength(2);
    expect(filterModelSelectorEntries(entries)).toHaveLength(2);
  });

  it("preserves runtime settings and tools when applying a model change", () => {
    const tool = {
      name: "read",
      description: "Read",
      inputSchema: { type: "object", properties: {} },
      executor: { kind: "native" as const, target: "read" },
      executionMode: "parallel" as const,
    };
    const initialConfig = {
      model: testModel("model-a"),
      provider: { apiKey: "old-key" },
      settings: {
        maxSteps: 3,
        parallelTools: true,
        allowToolCalls: true,
        allowApprovals: false,
        thinkingLevel: "high",
      },
      tools: [tool],
    };
    let appliedConfig = initialConfig;
    const app = {
      currentModel: initialConfig.model,
      currentProviderConfig: initialConfig.provider,
      currentThinkingLevel: "high",
      opts: { noTools: false },
      host: {
        getConfig: () => initialConfig,
        setConfig: (config: typeof initialConfig) => {
          appliedConfig = config;
        },
        setThinkingLevel: () => {},
      },
    };

    const next: ResolvedModel = {
      model: testModel("model-b"),
      providerConfig: { apiKey: "new-key" },
    };

    doApplyModelChange(app as never, next);

    expect(appliedConfig.model.id).toBe("model-b");
    expect(appliedConfig.provider).toEqual({ apiKey: "new-key" });
    expect(appliedConfig.settings).toMatchObject({
      maxSteps: 3,
      parallelTools: true,
      allowToolCalls: true,
      allowApprovals: false,
      thinkingLevel: "high",
    });
    expect(appliedConfig.tools).toEqual([tool]);
  });
});
