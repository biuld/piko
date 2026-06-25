import { describe, expect, it } from "bun:test";
import type { Model } from "piko-orch-protocol";
import { createHostConfig, PikoHost } from "../src/index.js";
import { fs, join, tmpdir } from "./bun-test-utils.js";
import { assistantText, FakeOrchd } from "./helpers/fake-orchd.js";

// ============================================================================
// Resource Invocation tests (Phase 5)
// ============================================================================

describe("Resource Invocation", () => {
  function resModel(id = "default-model"): Model<string> {
    return {
      id,
      name: `Model ${id}`,
      api: "openai-completions",
      provider: "openai",
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
model: openai/skill-model
---

# Test Skill

Use the special model for this skill.`,
    );

    const host = await PikoHost.create({
      config: createHostConfig(resModel("default-model"), undefined, {
        allowToolCalls: false,
      }),
      session: { cwd },
      orchestrator: new FakeOrchd([assistantText("Skill executed")]),
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

    const host = await PikoHost.create({
      config: createHostConfig(resModel(), undefined, {
        allowToolCalls: false,
      }),
      session: { cwd },
      orchestrator: new FakeOrchd([assistantText("Deep thought")]),
    });

    expect(host.getThinkingLevel()).toBe("off");

    const result = await host.runSkill("think-skill");

    expect(result.status).toBe("completed");
    expect(host.getThinkingLevel()).toBe("off");
  });

  it("should throw for unknown skill", async () => {
    const host = await PikoHost.create({
      config: createHostConfig(resModel()),
      orchestrator: new FakeOrchd(),
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

    const host = await PikoHost.create({
      config: createHostConfig(resModel(), undefined, {
        allowToolCalls: false,
      }),
      session: { cwd },
      orchestrator: new FakeOrchd([assistantText("Tools-restricted skill run")]),
    });

    expect(host.getActiveToolNames()).toBeUndefined();

    const result = await host.runSkill("tools-skill");

    expect(result.status).toBe("completed");
    // After skill completes, active tools should be restored
    expect(host.getActiveToolNames()).toBeUndefined();
  });
});
