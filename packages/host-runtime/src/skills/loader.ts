/**
 * Skills loader — scans directories for skill `.md` files,
 * parses YAML frontmatter, and returns Skill objects.
 *
 * Discovery rules:
 * - If a directory contains SKILL.md, treat it as a skill root
 * - Otherwise, load .md files directly and recurse into subdirectories
 * - .gitignore / .ignore patterns are respected
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve, sep } from "node:path";
import { getPikoDir } from "../session/index.js";
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

function loadSkillFromFile(filePath: string): {
  skill: Skill | null;
  diagnostics: SkillDiagnostic[];
} {
  const diagnostics: SkillDiagnostic[] = [];

  try {
    const rawContent = readFileSync(filePath, "utf-8");
    const { frontmatter } = parseFrontmatter<SkillFrontmatter>(rawContent);
    const skillDir = resolve(filePath, "..");
    const parentDirName = resolve(skillDir).split(sep).pop() ?? "unnamed";

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

function scanDirectory(dir: string, includeRootFiles: boolean): LoadSkillsResult {
  const skills: Skill[] = [];
  const diagnostics: SkillDiagnostic[] = [];
  const seenNames = new Set<string>();

  if (!existsSync(dir)) {
    return { skills, diagnostics };
  }

  let entries: import("node:fs").Dirent[];
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return { skills, diagnostics };
  }

  // Check for SKILL.md first — if found, don't recurse
  for (const entry of entries) {
    if (entry.name !== "SKILL.md") continue;

    const fullPath = join(dir, entry.name);
    const isFile = entry.isFile() || (entry.isSymbolicLink() && statSync(fullPath).isFile());
    if (!isFile) continue;

    const result = loadSkillFromFile(fullPath);
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

    const fullPath = join(dir, entry.name);

    if (entry.isDirectory() || (entry.isSymbolicLink() && statSync(fullPath).isDirectory())) {
      const subResult = scanDirectory(fullPath, false);
      for (const s of subResult.skills) {
        if (seenNames.has(s.name)) continue;
        seenNames.add(s.name);
        skills.push(s);
      }
      diagnostics.push(...subResult.diagnostics);
      continue;
    }

    if (!includeRootFiles || !entry.name.endsWith(".md")) continue;

    const isFile = entry.isFile() || (entry.isSymbolicLink() && statSync(fullPath).isFile());
    if (!isFile) continue;

    const result = loadSkillFromFile(fullPath);
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
export function loadSkills(options: LoadSkillsOptions): LoadSkillsResult {
  const { cwd } = options;
  const resolvedCwd = resolve(cwd);

  const projectDir = resolve(resolvedCwd, CONFIG_DIR_NAME, "skills");
  const globalDir = join(getPikoDir(), "skills");

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
  merge(scanDirectory(projectDir, true));
  // Then global skills
  merge(scanDirectory(globalDir, true));

  return {
    skills: Array.from(skillMap.values()),
    diagnostics: allDiagnostics,
  };
}
