import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { getPikoDir } from "../src/session/index.js";
import { SettingsManager } from "../src/settings/manager.js";
import { fs, join, tmpdir } from "./bun-test-utils.js";

describe("SettingsManager", () => {
  let tempCwd: string;
  let savedHome: string | undefined;

  beforeEach(() => {
    savedHome = process.env.HOME;
    // Set up a temporary home and cwd for settings files
    const homeDir = fs.mkdtempSync(join(tmpdir(), "piko-home-test-"));
    process.env.HOME = homeDir;

    tempCwd = fs.mkdtempSync(join(tmpdir(), "piko-cwd-test-"));
    fs.mkdirSync(join(tempCwd, ".piko"), { recursive: true });
    fs.mkdirSync(getPikoDir(), { recursive: true });
  });

  afterEach(() => {
    process.env.HOME = savedHome;
    try {
      if (tempCwd) fs.rmSync(tempCwd, { recursive: true, force: true });
    } catch {}
  });

  test("inMemory creates settings with overrides", () => {
    const manager = SettingsManager.inMemory({
      defaultModel: "model-in-memory",
      compaction: { enabled: false },
    });
    expect(manager.getDefaultModel()).toBe("model-in-memory");
    expect(manager.getCompactionSettings().enabled).toBe(false);
  });

  test("applies default hardcoded settings when files do not exist", async () => {
    const manager = await SettingsManager.create(tempCwd);
    expect(manager.getCompactionSettings()).toEqual({
      enabled: true,
      reserveTokens: 16384,
      keepRecentTokens: 20000,
    });
    expect(manager.getBranchSummarySettings()).toEqual({
      reserveTokens: 16384,
      skipPrompt: false,
    });
    expect(manager.getRetrySettings()).toEqual({
      enabled: true,
      maxRetries: 3,
      baseDelayMs: 2000,
    });
    expect(manager.getHideThinkingBlock()).toBe(false);
    expect(manager.getQuietStartup()).toBe(false);
    expect(manager.getClearOnShrink()).toBe(true);
  });

  test("merges settings layers in correct precedence", async () => {
    // 1. Create global settings
    const globalPath = join(getPikoDir(), "settings.json");
    fs.writeFileSync(
      globalPath,
      JSON.stringify({
        defaultProvider: "global-provider",
        defaultModel: "global-model",
        compaction: {
          reserveTokens: 100, // global override
        },
      }),
    );

    // 2. Create project settings
    const projectPath = join(tempCwd, ".piko", "settings.json");
    fs.writeFileSync(
      projectPath,
      JSON.stringify({
        defaultModel: "project-model", // overrides global
        theme: "project-theme",
        compaction: {
          keepRecentTokens: 500, // overrides default but preserves reserveTokens from global
        },
      }),
    );

    const manager = await SettingsManager.create(tempCwd);

    // Verify merging logic
    expect(manager.getDefaultProvider()).toBe("global-provider");
    expect(manager.getDefaultModel()).toBe("project-model");
    expect(manager.getTheme()).toBe("project-theme");
    expect(manager.getCompactionSettings()).toEqual({
      enabled: true, // default
      reserveTokens: 100, // global
      keepRecentTokens: 500, // project
    });
  });

  test("applyOverrides merges settings overrides correctly", async () => {
    const manager = await SettingsManager.create(tempCwd);
    manager.applyOverrides({
      defaultModel: "override-model",
      compaction: { reserveTokens: 999 },
    });

    expect(manager.getDefaultModel()).toBe("override-model");
    expect(manager.getCompactionSettings().reserveTokens).toBe(999);
  });

  test("accessors retrieve correct default or overridden values", () => {
    const manager = SettingsManager.inMemory({
      defaultProvider: "test-prov",
      defaultModel: "test-model",
      defaultThinkingLevel: "high",
      transport: "sse",
      theme: "dark",
      hideThinkingBlock: true,
      sessionDir: "/path/to/sessions",
      extensions: ["ext-1"],
      skills: ["skill-1"],
      prompts: ["prompt-1"],
      themes: ["theme-1"],
      enabledModels: ["model-a"],
      doubleEscapeAction: "fork",
      shellPath: "/bin/bash",
      sandbox: { enabled: true, policyPath: "policy.json", binaryPath: "/bin/sandbox" },
      quietStartup: true,
      clearOnShrink: false,
      steeringMode: "one-at-a-time",
      followUpMode: "all",
    });

    expect(manager.getSandboxSettings()).toEqual({
      enabled: true,
      policyPath: "policy.json",
      binaryPath: "/bin/sandbox",
    });

    expect(manager.getDefaultProvider()).toBe("test-prov");
    expect(manager.getDefaultModel()).toBe("test-model");
    expect(manager.getDefaultThinkingLevel()).toBe("high");
    expect(manager.getTransport()).toBe("sse");
    expect(manager.getTheme()).toBe("dark");
    expect(manager.getHideThinkingBlock()).toBe(true);
    expect(manager.getSessionDir()).toBe("/path/to/sessions");
    expect(manager.getExtensionPaths()).toEqual(["ext-1"]);
    expect(manager.getSkillPaths()).toEqual(["skill-1"]);
    expect(manager.getPromptTemplatePaths()).toEqual(["prompt-1"]);
    expect(manager.getThemePaths()).toEqual(["theme-1"]);
    expect(manager.getEnabledModels()).toEqual(["model-a"]);
    expect(manager.getDoubleEscapeAction()).toBe("fork");
    expect(manager.getShellPath()).toBe("/bin/bash");
    expect(manager.getQuietStartup()).toBe(true);
    expect(manager.getClearOnShrink()).toBe(false);
    expect(manager.getSteeringMode()).toBe("one-at-a-time");
    expect(manager.getFollowUpMode()).toBe("all");

    expect(manager.settings.defaultModel).toBe("test-model");
    expect(manager.getGlobalSettings()).toEqual({});
    expect(manager.getProjectSettings()).toEqual({});
  });

  test("persists changes to global settings file", async () => {
    const manager = await SettingsManager.create(tempCwd);

    manager.setDefaultModel("new-model");
    manager.setDefaultProvider("new-provider");
    manager.setDefaultModelAndProvider("provider-x", "model-x");
    manager.setDefaultThinkingLevel("low");
    manager.setTransport("auto");
    manager.setTheme("light");
    manager.setCompactionEnabled(false);
    manager.setCompactionReserveTokens(1000);
    manager.setCompactionKeepRecentTokens(2000);
    manager.setRetryEnabled(false);
    manager.setRetryMaxRetries(5);
    manager.setHideThinkingBlock(true);
    manager.setEnabledModels(["*"]);
    manager.setDoubleEscapeAction("none");
    manager.setShellPath("/bin/zsh");
    manager.setSessionDir("/some/dir");
    manager.setQuietStartup(true);
    manager.setClearOnShrink(false);
    manager.setSteeringMode("all");
    manager.setFollowUpMode("one-at-a-time");

    // Verify in-memory changes
    expect(manager.getDefaultModel()).toBe("model-x");
    expect(manager.getDefaultProvider()).toBe("provider-x");
    expect(manager.getDefaultThinkingLevel()).toBe("low");
    expect(manager.getTransport()).toBe("auto");
    expect(manager.getTheme()).toBe("light");
    expect(manager.getCompactionSettings()).toEqual({
      enabled: false,
      reserveTokens: 1000,
      keepRecentTokens: 2000,
    });
    expect(manager.getRetrySettings()).toEqual({
      enabled: false,
      maxRetries: 5,
      baseDelayMs: 2000, // baseDelayMs preserved from default
    });
    expect(manager.getHideThinkingBlock()).toBe(true);
    expect(manager.getEnabledModels()).toEqual(["*"]);
    expect(manager.getDoubleEscapeAction()).toBe("none");
    expect(manager.getShellPath()).toBe("/bin/zsh");
    expect(manager.getSessionDir()).toBe("/some/dir");
    expect(manager.getQuietStartup()).toBe(true);
    expect(manager.getClearOnShrink()).toBe(false);
    expect(manager.getSteeringMode()).toBe("all");
    expect(manager.getFollowUpMode()).toBe("one-at-a-time");

    // Verify global settings file has been written and contains correct parameters
    await manager.flush();
    const globalPath = join(getPikoDir(), "settings.json");
    const content = JSON.parse(fs.readFileSync(globalPath, "utf-8"));

    expect(content.defaultModel).toBe("model-x");
    expect(content.defaultProvider).toBe("provider-x");
    expect(content.defaultThinkingLevel).toBe("low");
    expect(content.transport).toBe("auto");
    expect(content.theme).toBe("light");
    expect(content.compaction?.enabled).toBe(false);
    expect(content.compaction?.reserveTokens).toBe(1000);
    expect(content.compaction?.keepRecentTokens).toBe(2000);
    expect(content.retry?.enabled).toBe(false);
    expect(content.retry?.maxRetries).toBe(5);
    expect(content.hideThinkingBlock).toBe(true);
    expect(content.enabledModels).toEqual(["*"]);
    expect(content.doubleEscapeAction).toBe("none");
    expect(content.shellPath).toBe("/bin/zsh");
    expect(content.sessionDir).toBe("/some/dir");
    expect(content.quietStartup).toBe(true);
    expect(content.clearOnShrink).toBe(false);
    expect(content.steeringMode).toBe("all");
    expect(content.followUpMode).toBe("one-at-a-time");
  });

  test("reload loads latest changes from disk", async () => {
    const projectPath = join(tempCwd, ".piko", "settings.json");
    fs.writeFileSync(
      projectPath,
      JSON.stringify({
        defaultModel: "initial-model",
      }),
    );

    // Force load the initial-model by re-initializing or manually testing reload
    const manager2 = await SettingsManager.create(tempCwd);
    expect(manager2.getDefaultModel()).toBe("initial-model");

    // Now write a new model directly to disk
    fs.writeFileSync(
      projectPath,
      JSON.stringify({
        defaultModel: "updated-model",
      }),
    );

    // Before reload, it should still have the cached initial-model
    expect(manager2.getDefaultModel()).toBe("initial-model");

    // Reload and check
    await manager2.reload();
    expect(manager2.getDefaultModel()).toBe("updated-model");
  });
});
