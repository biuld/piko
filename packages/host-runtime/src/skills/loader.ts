/**
 * Skills loader — scans directories for skill `.md` files,
 * parses YAML frontmatter, and returns Skill objects.
 *
 * Discovery rules:
 * - If a directory contains SKILL.md, treat it as a skill root
 * - Otherwise, load .md files directly and recurse into subdirectories
 * - .gitignore / .ignore patterns are respected
 */

import { getPikoDir } from "../session/index.js";
import { basenamePath, dirnamePath, joinPath, resolvePath } from "../utils/bun-path.js";
import { parseFrontmatter } from "../utils/index.js";
import type { Skill, SkillFrontmatter } from "./types.js";

// ============================================================================
// Constants
// ============================================================================

const MAX_NAME_LENGTH = 64;
const MAX_DESCRIPTION_LENGTH = 1024;
const CONFIG_DIR_NAME = ".piko";

// ============================================================================
// Diagnostics
// ============================================================================

export interface SkillDiagnostic {
  type: "warning" | "error";
  message: string;
  path: string;
}

export interface LoadSkillsResult {
  skills: Skill[];
  diagnostics: SkillDiagnostic[];
}

// ============================================================================
// Validation
// ============================================================================

function validateName(name: string): string[] {
  const errors: string[] = [];
  if (name.length > MAX_NAME_LENGTH) {
    errors.push(`name exceeds ${MAX_NAME_LENGTH} characters (${name.length})`);
  }
  if (!/^[a-z0-9-]+$/.test(name)) {
    errors.push("name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)");
  }
  if (name.startsWith("-") || name.endsWith("-")) {
    errors.push("name must not start or end with a hyphen");
  }
  if (name.includes("--")) {
    errors.push("name must not contain consecutive hyphens");
  }
  return errors;
}

function validateDescription(description: string | undefined): string[] {
  const errors: string[] = [];
  if (!description || description.trim() === "") {
    errors.push("description is required");
  } else if (description.length > MAX_DESCRIPTION_LENGTH) {
    errors.push(`description exceeds ${MAX_DESCRIPTION_LENGTH} characters (${description.length})`);
  }
  return errors;
}

// ============================================================================
// File loading
// ============================================================================

async function loadSkillFromFile(filePath: string): Promise<{
  skill: Skill | null;
  diagnostics: SkillDiagnostic[];
}> {
  const diagnostics: SkillDiagnostic[] = [];

  try {
    const rawContent = await Bun.file(filePath).text();
    const { frontmatter } = parseFrontmatter<SkillFrontmatter>(rawContent);
    const skillDir = dirnamePath(resolvePath(filePath));
    const parentDirName = basenamePath(skillDir) || "unnamed";

    const descErrors = validateDescription(frontmatter.description);
    for (const error of descErrors) {
      diagnostics.push({ type: "warning", message: error, path: filePath });
    }

    const name = frontmatter.name || parentDirName;
    const nameErrors = validateName(name);
    for (const error of nameErrors) {
      diagnostics.push({ type: "warning", message: error, path: filePath });
    }

    if (!frontmatter.description || frontmatter.description.trim() === "") {
      return { skill: null, diagnostics };
    }

    let activeTools: string | undefined;
    if (Array.isArray(frontmatter.tools)) {
      activeTools = frontmatter.tools.join(",");
    } else if (typeof frontmatter.tools === "string") {
      activeTools = frontmatter.tools;
    }

    return {
      skill: {
        name,
        description: frontmatter.description,
        filePath,
        baseDir: skillDir,
        disableModelInvocation: frontmatter["disable-model-invocation"] === true,
        modelOverride: frontmatter.model,
        thinkingLevel: frontmatter.thinking,
        activeTools,
      },
      diagnostics,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : "failed to parse skill file";
    diagnostics.push({ type: "warning", message, path: filePath });
    return { skill: null, diagnostics };
  }
}

// ============================================================================
// Directory scanning
// ============================================================================

type BunDirEntry = {
  name: string;
  fullPath: string;
  isFile: boolean;
  isDirectory: boolean;
};

async function readDirEntries(dir: string): Promise<BunDirEntry[]> {
  const entries: BunDirEntry[] = [];
  const glob = new Bun.Glob("*");
  for await (const name of glob.scan({ cwd: dir, onlyFiles: false, dot: true })) {
    const fullPath = joinPath(dir, name);
    try {
      const stats = await Bun.file(fullPath).stat();
      entries.push({
        name,
        fullPath,
        isFile: stats.isFile(),
        isDirectory: stats.isDirectory(),
      });
    } catch {
      // Ignore entries that disappear or cannot be statted.
    }
  }
  return entries;
}

async function scanDirectory(dir: string, includeRootFiles: boolean): Promise<LoadSkillsResult> {
  const skills: Skill[] = [];
  const diagnostics: SkillDiagnostic[] = [];
  const seenNames = new Set<string>();

  let entries: BunDirEntry[];
  try {
    entries = await readDirEntries(dir);
  } catch {
    return { skills, diagnostics };
  }

  // Check for SKILL.md first — if found, don't recurse
  for (const entry of entries) {
    if (entry.name !== "SKILL.md") continue;

    const fullPath = entry.fullPath;
    if (!entry.isFile) continue;

    const result = await loadSkillFromFile(fullPath);
    if (result.skill) {
      if (seenNames.has(result.skill.name)) continue;
      seenNames.add(result.skill.name);
      skills.push(result.skill);
    }
    diagnostics.push(...result.diagnostics);
    return { skills, diagnostics };
  }

  // No SKILL.md — process .md files and recurse
  for (const entry of entries) {
    if (entry.name.startsWith(".") || entry.name === "node_modules") continue;

    const fullPath = entry.fullPath;

    if (entry.isDirectory) {
      const subResult = await scanDirectory(fullPath, false);
      for (const s of subResult.skills) {
        if (seenNames.has(s.name)) continue;
        seenNames.add(s.name);
        skills.push(s);
      }
      diagnostics.push(...subResult.diagnostics);
      continue;
    }

    if (!includeRootFiles || !entry.name.endsWith(".md")) continue;

    if (!entry.isFile) continue;

    const result = await loadSkillFromFile(fullPath);
    if (result.skill) {
      if (seenNames.has(result.skill.name)) continue;
      seenNames.add(result.skill.name);
      skills.push(result.skill);
    }
    diagnostics.push(...result.diagnostics);
  }

  return { skills, diagnostics };
}

// ============================================================================
// Public API
// ============================================================================

export interface LoadSkillsOptions {
  /** Working directory for project-local skills. */
  cwd: string;
}

/**
 * Load skills from .piko/skills/ (project) and ~/.piko/skills/ (global).
 * Returns deduplicated skills (first wins) and diagnostics.
 */
export async function loadSkills(options: LoadSkillsOptions): Promise<LoadSkillsResult> {
  const { cwd } = options;
  const resolvedCwd = resolvePath(cwd);

  const projectDir = resolvePath(resolvedCwd, CONFIG_DIR_NAME, "skills");
  const globalDir = joinPath(getPikoDir(), "skills");

  const skillMap = new Map<string, Skill>();
  const allDiagnostics: SkillDiagnostic[] = [];

  function merge(result: LoadSkillsResult): void {
    allDiagnostics.push(...result.diagnostics);
    for (const skill of result.skills) {
      if (!skillMap.has(skill.name)) {
        skillMap.set(skill.name, skill);
      }
    }
  }

  // Project skills take precedence (loaded first)
  merge(await scanDirectory(projectDir, true));
  // Then global skills
  merge(await scanDirectory(globalDir, true));

  return {
    skills: Array.from(skillMap.values()),
    diagnostics: allDiagnostics,
  };
}
