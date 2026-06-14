import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import * as fs from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { discoverResources } from "../src/resource-loader.js";

describe("discoverResources", () => {
  let savedHome: string | undefined;
  let tempHome: string;
  let tempCwd: string;

  beforeEach(() => {
    savedHome = process.env.HOME;
    tempHome = fs.mkdtempSync(join(tmpdir(), "piko-global-resource-test-"));
    process.env.HOME = tempHome;

    tempCwd = fs.mkdtempSync(join(tmpdir(), "piko-project-resource-test-"));
  });

  afterEach(() => {
    process.env.HOME = savedHome;
    try {
      fs.rmSync(tempHome, { recursive: true, force: true });
      fs.rmSync(tempCwd, { recursive: true, force: true });
    } catch {}
  });

  it("handles non-existent global and project directories", () => {
    const res = discoverResources(tempCwd);
    expect(res.skillDirs).toHaveLength(0);
    expect(res.promptDirs).toHaveLength(0);
    expect(res.themeDirs).toHaveLength(0);
    expect(res.contextFilePaths).toHaveLength(0);
  });

  it("discovers skills, prompts, themes, and context files in global and project directories", () => {
    // Set up global .piko dir
    const globalPiko = join(tempHome, ".piko");
    fs.mkdirSync(join(globalPiko, "skills"), { recursive: true });
    fs.mkdirSync(join(globalPiko, "prompts"), { recursive: true });
    fs.writeFileSync(join(globalPiko, "AGENTS.md"), "global agents", "utf-8");

    // Set up project .piko dir
    const projectPiko = join(tempCwd, ".piko");
    fs.mkdirSync(join(projectPiko, "skills"), { recursive: true });
    fs.mkdirSync(join(projectPiko, "themes"), { recursive: true });
    fs.writeFileSync(join(projectPiko, "CLAUDE.md"), "project claude", "utf-8");

    const res = discoverResources(tempCwd);

    expect(res.skillDirs).toContain(join(globalPiko, "skills"));
    expect(res.skillDirs).toContain(join(projectPiko, "skills"));

    expect(res.promptDirs).toContain(join(globalPiko, "prompts"));
    expect(res.promptDirs).not.toContain(join(projectPiko, "prompts"));

    expect(res.themeDirs).not.toContain(join(globalPiko, "themes"));
    expect(res.themeDirs).toContain(join(projectPiko, "themes"));

    expect(res.contextFilePaths).toContain(join(globalPiko, "AGENTS.md"));
    expect(res.contextFilePaths).toContain(join(projectPiko, "CLAUDE.md"));
  });
});
