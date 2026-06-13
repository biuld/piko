import { afterAll, beforeAll, describe, expect, it } from "bun:test";
import * as fs from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FauxProviderRegistration, Model } from "@earendil-works/pi-ai";
import { fauxAssistantMessage, registerFauxProvider } from "@earendil-works/pi-ai";
import { createNativeModelExecutor } from "piko-orchestrator";
import { createHostConfig, PikoHost } from "../src/index.js";

// ============================================================================
// Resource Invocation tests (Phase 5)
// ============================================================================

describe("Resource Invocation", () => {
  let faux: FauxProviderRegistration;

  beforeAll(() => {
    faux = registerFauxProvider({
      api: "openai-completions",
      provider: "faux-resource",
      models: [{ id: "default-model" }, { id: "skill-model" }],
    });
  });

  afterAll(() => {
    faux?.unregister();
  });

  function resModel(id = "default-model"): Model<string> {
    return {
      id,
      name: `Model ${id}`,
      api: "openai-completions",
      provider: "faux-resource",
      baseUrl: "http://localhost:0",
      reasoning: false,
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: 128000,
      maxTokens: 16384,
    };
  }

  it("should apply skill model override", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-resource-cwd-"));

    const skillsDir = join(cwd, ".piko", "skills", "test-skill");
    await fs.mkdir(skillsDir, { recursive: true });
    await fs.writeFile(
      join(skillsDir, "SKILL.md"),
      `---
name: test-skill
description: A test skill with model override
model: faux-resource/skill-model
---

# Test Skill

Use the special model for this skill.`,
    );

    faux.setResponses([fauxAssistantMessage("Skill executed")]);

    const host = await PikoHost.create({
      engine: createNativeModelExecutor(),
      config: createHostConfig(resModel("default-model"), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
      session: { cwd },
    });

    expect(host.getConfig().model.id).toBe("default-model");

    const result = await host.runSkill("test-skill");

    expect(result.status).toBe("completed");
    expect(host.getConfig().model.id).toBe("default-model");
  });

  it("should apply skill thinking level override", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-resource-think-"));

    const skillsDir = join(cwd, ".piko", "skills", "think-skill");
    await fs.mkdir(skillsDir, { recursive: true });
    await fs.writeFile(
      join(skillsDir, "SKILL.md"),
      `---
name: think-skill
description: A skill with thinking override
thinking: high
---

# Thinking Skill

Think hard about this.`,
    );

    faux.setResponses([fauxAssistantMessage("Deep thought")]);

    const host = await PikoHost.create({
      engine: createNativeModelExecutor(),
      config: createHostConfig(resModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
      session: { cwd },
    });

    expect(host.getThinkingLevel()).toBe("off");

    const result = await host.runSkill("think-skill");

    expect(result.status).toBe("completed");
    expect(host.getThinkingLevel()).toBe("off");
  });

  it("should throw for unknown skill", async () => {
    const host = await PikoHost.create({
      engine: createNativeModelExecutor(),
      config: createHostConfig(resModel()),
    });
    await expect(host.runSkill("nonexistent")).rejects.toThrow("Unknown skill");
  });

  it("should apply skill active tools override", async () => {
    const cwd = await fs.mkdtemp(join(tmpdir(), "piko-resource-tools-"));

    const skillsDir = join(cwd, ".piko", "skills", "tools-skill");
    await fs.mkdir(skillsDir, { recursive: true });
    await fs.writeFile(
      join(skillsDir, "SKILL.md"),
      `---
name: tools-skill
description: A test skill with active tools restriction
tools: read, edit
---

# Tools Skill

Only read and edit tools are available.`,
    );

    faux.setResponses([fauxAssistantMessage("Tools-restricted skill run")]);

    const host = await PikoHost.create({
      engine: createNativeModelExecutor(),
      config: createHostConfig(resModel(), undefined, {
        allowToolCalls: false,
        maxSteps: 5,
      }),
      session: { cwd },
    });

    expect(host.getActiveToolNames()).toBeUndefined();

    const result = await host.runSkill("tools-skill");

    expect(result.status).toBe("completed");
    // After skill completes, active tools should be restored
    expect(host.getActiveToolNames()).toBeUndefined();
  });
});
