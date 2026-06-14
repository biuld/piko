import { describe, expect, it } from "bun:test";
import { AuthStorage } from "../src/auth/index.js";
import { findModel, listAvailableModels } from "../src/models/loader.js";
import { ModelRegistry } from "../src/models/registry.js";

describe("Models System", () => {
  describe("loader (loader.ts)", () => {
    it("listAvailableModels returns all registered providers and models", () => {
      const list = listAvailableModels();
      const providers = list.map((p) => p.provider);
      expect(providers).toContain("openai");
      expect(providers).toContain("anthropic");

      const openaiInfo = list.find((p) => p.provider === "openai");
      expect(openaiInfo).toBeDefined();
      expect(openaiInfo!.models.length).toBeGreaterThan(0);
    });

    it("findModel returns correct model when both id and provider are matched", () => {
      const res = findModel("gpt-4", "openai");
      expect(res).not.toBeNull();
      expect(res!.model.id).toBe("gpt-4");
      expect(res!.model.provider).toBe("openai");
    });

    it("findModel falls back to scan all providers when provider is specified but model is under a different provider", () => {
      const res = findModel("gpt-4", "anthropic");
      expect(res).not.toBeNull();
      expect(res!.model.id).toBe("gpt-4");
      expect(["openai", "azure-openai-responses", "azure-openai"]).toContain(res!.model.provider);
    });

    it("findModel finds model by ID only", () => {
      const res = findModel("gpt-4");
      expect(res).not.toBeNull();
      expect(res!.model.id).toBe("gpt-4");
      expect(["openai", "azure-openai-responses", "azure-openai"]).toContain(res!.model.provider);
    });

    it("findModel returns first model of provider when provider name only is passed", () => {
      const res = findModel(undefined, "openai");
      expect(res).not.toBeNull();
      expect(res!.model.provider).toBe("openai");
    });

    it("findModel falls back to defaults when no arguments are passed", () => {
      const res = findModel();
      expect(res).not.toBeNull();
      expect(["claude-sonnet-4-5-20250929", "gpt-4o"]).toContain(res!.model.id);
    });

    it("findModel falls back to default/first available for nonexistent model and provider", () => {
      const res = findModel("nonexistent-model", "nonexistent-provider");
      expect(res).not.toBeNull();
      expect(res!.model).toBeDefined();
    });
  });

  describe("registry (registry.ts)", () => {
    let authStorage: AuthStorage;
    let registry: ModelRegistry;

    authStorage = AuthStorage.inMemory();
    authStorage.set("openai", { type: "api_key", key: "sk-openai-test" });
    registry = new ModelRegistry(authStorage);

    it("getAuthStorage returns the auth storage instance", () => {
      expect(registry.getAuthStorage()).toBe(authStorage);
    });

    it("hasAuth returns true if provider has auth", () => {
      expect(registry.hasAuth("openai")).toBe(true);
      expect(registry.hasAuth("anthropic")).toBe(false);
    });

    it("listProviders lists providers and their models", () => {
      const list = registry.listProviders();
      const providers = list.map((p) => p.provider);
      expect(providers).toContain("openai");
      expect(providers).toContain("anthropic");
    });

    it("listModels lists all models", () => {
      const models = registry.listModels();
      const ids = models.map((m) => m.id);
      expect(ids).toContain("gpt-4");
    });

    it("listScopedModels returns all models when scope is empty", () => {
      const all = registry.listModels();
      const scoped = registry.listScopedModels();
      expect(scoped.length).toBe(all.length);
    });

    it("listScopedModels filters by provider scope", () => {
      registry.setScopedModels(["openai"]);
      const scoped = registry.listScopedModels();
      const providers = new Set(scoped.map((m) => m.provider));
      expect(providers.has("openai")).toBe(true);
      expect(providers.has("anthropic")).toBe(false);
    });

    it("listScopedModels filters by provider/model scope pattern", () => {
      registry.setScopedModels(["openai/gpt-4o-mini", "anthropic/claude-3-5-sonnet-20241022"]);
      const scoped = registry.listScopedModels();
      const ids = scoped.map((m) => m.id);
      expect(ids).toContain("gpt-4o-mini");
      expect(ids).toContain("claude-3-5-sonnet-20241022");
      expect(ids).not.toContain("gpt-4");
    });

    it("resolve resolves model with both provider and ID", () => {
      const res = registry.resolve("gpt-4", "openai");
      expect(res).not.toBeNull();
      expect(res!.model.id).toBe("gpt-4");
      expect(res!.providerConfig.apiKey).toBe("sk-openai-test");
    });

    it("resolve falls back when provider does not match model id", () => {
      const res = registry.resolve("gpt-4", "anthropic");
      expect(res).not.toBeNull();
      expect(res!.model.id).toBe("gpt-4");
    });

    it("resolve resolves model by ID only", () => {
      const res = registry.resolve("gpt-4");
      expect(res).not.toBeNull();
      expect(res!.model.id).toBe("gpt-4");
    });

    it("resolve resolves model by provider name only", () => {
      const res = registry.resolve(undefined, "openai");
      expect(res).not.toBeNull();
      expect(res!.model.provider).toBe("openai");
    });

    it("resolve falls back to default models", () => {
      const res = registry.resolve();
      expect(res).not.toBeNull();
      expect(["claude-sonnet-4-5-20250929", "gpt-4o"]).toContain(res!.model.id);
    });

    it("resolve falls back to defaults for completely unknown query", () => {
      const res = registry.resolve("nonexistent-model", "nonexistent-provider");
      expect(res).not.toBeNull();
      expect(res!.model).toBeDefined();
    });
  });
});
