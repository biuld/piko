import { describe, expect, test } from "bun:test";

import { restoreRuntimeFromSession } from "../src/host/runtime-config/index.js";

const PROVIDER = "openai";
const MODEL_ID = "gpt-4";

describe("restoreRuntimeFromSession", () => {
  test("returns empty when session has no relevant entries", async () => {
    const mockSession = {
      getBranch: async () => [],
    } as any;

    const currentConfig = {
      provider: { apiKey: "key-123" },
      settings: {},
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
      settings: {},
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
      settings: {},
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
      settings: {},
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
