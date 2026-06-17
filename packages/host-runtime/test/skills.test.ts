import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import * as fs from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { getPikoDir } from "../src/session/index.js";
import { formatSkillsForPrompt } from "../src/skills/formatter.js";
import { loadSkills } from "../src/skills/loader.js";
import type { Skill } from "../src/skills/types.js";

describe("Skills", () => {
  let tempCwd: string;
  let savedHome: string | undefined;

  beforeEach(() => {
    savedHome = process.env.HOME;
    const homeDir = fs.mkdtempSync(join(tmpdir(), "piko-skills-home-"));
    process.env.HOME = homeDir;

    tempCwd = fs.mkdtempSync(join(tmpdir(), "piko-skills-cwd-"));
    fs.mkdirSync(join(tempCwd, ".piko", "skills"), { recursive: true });
    fs.mkdirSync(join(getPikoDir(), "skills"), { recursive: true });
  });

  afterEach(() => {
    process.env.HOME = savedHome;
    try {
      if (tempCwd) fs.rmSync(tempCwd, { recursive: true, force: true });
    } catch {}
  });

  describe("formatSkillsForPrompt", () => {
    test("returns empty string when no skills provided", () => {
      expect(formatSkillsForPrompt([])).toBe("");
    });

    test("formats visible skills and filters out disabled ones", () => {
      const skills: Skill[] = [
        {
          name: "skill-a",
          description: "This is skill A & B",
          filePath: "/path/to/skill-a.md",
          baseDir: "/path/to",
          disableModelInvocation: false,
        },
        {
          name: "skill-b",
          description: "This is skill B <hidden>",
          filePath: "/path/to/skill-b.md",
          baseDir: "/path/to",
          disableModelInvocation: true, // should be excluded
        },
      ];

      const formatted = formatSkillsForPrompt(skills);
      expect(formatted).toContain("<available_skills>");
      expect(formatted).toContain("<name>skill-a</name>");
      expect(formatted).toContain("<description>This is skill A &amp; B</description>");
      expect(formatted).not.toContain("skill-b");
    });
  });

  describe("loadSkills", () => {
    test("loads nothing when folders are empty", async () => {
      const result = await loadSkills({ cwd: tempCwd });
      expect(result.skills).toHaveLength(0);
      expect(result.diagnostics).toHaveLength(0);
    });

    test("loads a valid skill from .piko/skills/", async () => {
      const skillFile = join(tempCwd, ".piko", "skills", "my-skill.md");
      fs.writeFileSync(
        skillFile,
        `---
name: my-skill
description: "A great skill"
tools: [t1, t2]
---
# Content
`,
      );

      const result = await loadSkills({ cwd: tempCwd });
      expect(result.skills).toHaveLength(1);
      expect(result.skills[0].name).toBe("my-skill");
      expect(result.skills[0].description).toBe("A great skill");
      expect(result.skills[0].activeTools).toBe("t1,t2");
      expect(result.diagnostics).toHaveLength(0);
    });

    test("validates name length and formatting", async () => {
      const badNameFile = join(tempCwd, ".piko", "skills", "bad.md");
      fs.writeFileSync(
        badNameFile,
        `---
name: "too-long-name-exceeding-sixty-four-characters-limit-to-trigger-warning-errors"
description: "Desc"
---
`,
      );

      const invalidCharFile = join(tempCwd, ".piko", "skills", "invalid.md");
      fs.writeFileSync(
        invalidCharFile,
        `---
name: "Invalid_Name!"
description: "Desc"
---
`,
      );

      const hyphenStartFile = join(tempCwd, ".piko", "skills", "hyphen-start.md");
      fs.writeFileSync(
        hyphenStartFile,
        `---
name: "-bad-start-"
description: "Desc"
---
`,
      );

      const consecutiveHyphensFile = join(tempCwd, ".piko", "skills", "consecutive.md");
      fs.writeFileSync(
        consecutiveHyphensFile,
        `---
name: "bad--consecutive"
description: "Desc"
---
`,
      );

      const result = await loadSkills({ cwd: tempCwd });
      expect(result.diagnostics.length).toBeGreaterThanOrEqual(4);
      expect(result.diagnostics.some((d) => d.message.includes("exceeds 64 characters"))).toBe(
        true,
      );
      expect(result.diagnostics.some((d) => d.message.includes("invalid characters"))).toBe(true);
      expect(
        result.diagnostics.some((d) => d.message.includes("must not start or end with a hyphen")),
      ).toBe(true);
      expect(
        result.diagnostics.some((d) => d.message.includes("must not contain consecutive hyphens")),
      ).toBe(true);
    });

    test("validates description is required and length limit", async () => {
      const emptyDescFile = join(tempCwd, ".piko", "skills", "empty.md");
      fs.writeFileSync(
        emptyDescFile,
        `---
name: ok-name
---
`,
      );

      const longDescFile = join(tempCwd, ".piko", "skills", "long-desc.md");
      fs.writeFileSync(
        longDescFile,
        `---
name: ok-name-2
description: "${"a".repeat(1025)}"
---
`,
      );

      const result = await loadSkills({ cwd: tempCwd });
      expect(result.diagnostics.some((d) => d.message.includes("description is required"))).toBe(
        true,
      );
      expect(
        result.diagnostics.some((d) => d.message.includes("description exceeds 1024 characters")),
      ).toBe(true);
    });

    test("handles file parsing errors gracefully", async () => {
      const malformedFile = join(tempCwd, ".piko", "skills", "malformed.md");
      // Writing invalid yaml that will crash parser
      fs.writeFileSync(
        malformedFile,
        `---
: invalid
---
`,
      );

      const result = await loadSkills({ cwd: tempCwd });
      expect(result.skills).toHaveLength(0);
      expect(result.diagnostics.some((d) => d.path === malformedFile)).toBe(true);
    });

    test("handles non-existent directories gracefully", async () => {
      // Direct call loadSkills with a non-existent cwd directory
      const result = await loadSkills({ cwd: "/non-existent-cwd-path-12345" });
      expect(result.skills).toHaveLength(0);
      expect(result.diagnostics).toHaveLength(0);
    });

    test("respects SKILL.md logic and does not recurse if present", async () => {
      const subDir = join(tempCwd, ".piko", "skills", "group");
      fs.mkdirSync(subDir);

      fs.writeFileSync(
        join(subDir, "SKILL.md"),
        `---
name: skill-md-parent
description: "Parent description"
---
`,
      );

      fs.writeFileSync(
        join(subDir, "other.md"),
        `---
name: skill-md-child
description: "Child description"
---
`,
      );

      const result = await loadSkills({ cwd: tempCwd });
      // Only the SKILL.md in the folder should be parsed. other.md is ignored
      expect(result.skills.map((s) => s.name)).toContain("skill-md-parent");
      expect(result.skills.map((s) => s.name)).not.toContain("skill-md-child");
    });

    test("deduplicates skills preferring project ones over global ones", async () => {
      // 1. Project skill
      fs.writeFileSync(
        join(tempCwd, ".piko", "skills", "duplicate.md"),
        `---
name: duplicate-skill
description: "Project version"
---
`,
      );

      // 2. Global skill with same name
      fs.writeFileSync(
        join(getPikoDir(), "skills", "duplicate.md"),
        `---
name: duplicate-skill
description: "Global version"
---
`,
      );

      const result = await loadSkills({ cwd: tempCwd });
      expect(result.skills).toHaveLength(1);
      expect(result.skills[0].description).toBe("Project version");
    });
  });
});
