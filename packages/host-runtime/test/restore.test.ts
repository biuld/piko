import { afterAll, beforeAll, describe, expect, mock, test } from "bun:test";
import { type FauxProviderRegistration, registerFauxProvider } from "@earendil-works/pi-ai";

mock.module("piko-orchestrator", () => {
  return {
    getModel: (provider: string, modelId: string) => {
      if (provider === "faux-restore" && modelId === "model-restore") {
        return {
          id: modelId,
          name: "Model Restore",
          api: "openai-completions",
          provider: provider,
          baseUrl: "http://localhost:0",
          reasoning: false,
          input: ["text"],
          cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
          contextWindow: 128000,
          maxTokens: 16384,
        };
      }
      return null;
    },
    getEnvApiKey: (_provider: string) => "env-key",
  };
});

import { restoreRuntimeFromSession } from "../src/host/runtime-config/index.js";

const PROVIDER = "faux-restore";
const API = "openai-completions";
const MODEL_ID = "model-restore";

describe("restoreRuntimeFromSession", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: API,
      provider: PROVIDER,
      models: [{ id: MODEL_ID }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  test("returns empty when session has no relevant entries", async () => {
    const mockSession = {
      getBranch: async () => [],
    } as any;

    const currentConfig = {
      provider: { apiKey: "key-123" },
      settings: { maxSteps: 5 },
      tools: [],
    } as any;

    const result = await restoreRuntimeFromSession(mockSession, currentConfig);
    expect(result.config).toBeNull();
    expect(result.thinkingLevel).toBeUndefined();
    expect(result.activeToolNames).toBeUndefined();
    expect(result.hasActiveToolsEntry).toBe(false);
  });

  test("correctly parses last model, thinking level, and active tools changes", async () => {
    const mockSession = {
      getBranch: async () => [
        { type: "model_change", provider: PROVIDER, modelId: MODEL_ID },
        { type: "thinking_level_change", thinkingLevel: "medium" },
        { type: "active_tools_change", activeToolNames: ["t1", "t2"] },
      ],
    } as any;

    const currentConfig = {
      provider: { apiKey: "key-123" },
      settings: { maxSteps: 5 },
      tools: [{ name: "t1" }],
    } as any;

    const result = await restoreRuntimeFromSession(mockSession, currentConfig);
    expect(result.config).not.toBeNull();
    expect(result.config?.model.id).toBe(MODEL_ID);
    expect(result.config?.provider.apiKey).toBe("key-123");
    expect(result.thinkingLevel).toBe("medium");
    expect(result.activeToolNames).toEqual(["t1", "t2"]);
    expect(result.hasActiveToolsEntry).toBe(true);
  });

  test("handles empty/cleared active tools", async () => {
    const mockSession = {
      getBranch: async () => [
        { type: "active_tools_change", activeToolNames: [] }, // empty array means clear
      ],
    } as any;

    const currentConfig = {
      provider: { apiKey: "key-123" },
      settings: { maxSteps: 5 },
      tools: [],
    } as any;

    const result = await restoreRuntimeFromSession(mockSession, currentConfig);
    expect(result.activeToolNames).toBeUndefined();
    expect(result.hasActiveToolsEntry).toBe(true);
  });

  test("uses toolNames if activeToolNames is missing in active_tools_change", async () => {
    const mockSession = {
      getBranch: async () => [{ type: "active_tools_change", toolNames: ["t3"] }],
    } as any;

    const currentConfig = {
      provider: { apiKey: "key-123" },
      settings: { maxSteps: 5 },
      tools: [],
    } as any;

    const result = await restoreRuntimeFromSession(mockSession, currentConfig);
    expect(result.activeToolNames).toEqual(["t3"]);
    expect(result.hasActiveToolsEntry).toBe(true);
  });

  test("ignores older entries when duplicate types exist (reads backwards)", async () => {
    const mockSession = {
      getBranch: async () => [
        { type: "thinking_level_change", thinkingLevel: "low" },
        { type: "thinking_level_change", thinkingLevel: "high" }, // this is later
      ],
    } as any;

    const currentConfig = {} as any;

    const result = await restoreRuntimeFromSession(mockSession, currentConfig);
    expect(result.thinkingLevel).toBe("high");
  });

  test("loads session persistence overview when available", async () => {
    const overview = {
      rootSessionId: "root",
      rootSessionPath: "/tmp/root.jsonl",
      mainMessageCount: 2,
      hasSidecar: true,
      agentSessions: [],
      tasks: [],
      subagentCount: 1,
      taskCount: 1,
    };
    const mockSession = {
      getBranch: async () => [],
      loadPersistenceOverview: async () => overview,
    } as any;

    const result = await restoreRuntimeFromSession(mockSession, {} as any);
    expect(result.sessionPersistenceOverview).toBe(overview);
  });

  test("returns null config if getModel fails or model does not exist", async () => {
    const mockSession = {
      getBranch: async () => [
        { type: "model_change", provider: "unknown", modelId: "nonexistent" },
      ],
    } as any;

    const currentConfig = {
      provider: {},
      settings: {},
      tools: [],
    } as any;

    const result = await restoreRuntimeFromSession(mockSession, currentConfig);
    expect(result.config).toBeNull();
  });

  test("handles errors gracefully inside sessionManager.getBranch", async () => {
    const mockSession = {
      getBranch: async () => {
        throw new Error("Failed to read database");
      },
    } as any;

    const result = await restoreRuntimeFromSession(mockSession, {} as any);
    expect(result.config).toBeNull();
    expect(result.thinkingLevel).toBeUndefined();
    expect(result.activeToolNames).toBeUndefined();
    expect(result.hasActiveToolsEntry).toBe(false);
  });
});
