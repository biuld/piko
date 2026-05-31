/**
 * Resource Loader — unified discovery of .piko resources.
 *
 * Scans project and global .piko/ directories to discover:
 * - Skills (.piko/skills/*.md)
 * - Prompt templates (.piko/prompts/*.md)
 * - Themes (.piko/themes/*.json)
 * - Context files (AGENTS.md, CLAUDE.md)
 *
 * De-duplicates by path, reports load errors as diagnostics.
 */

import { existsSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { getPikoDir } from "./session/index.js";

// ============================================================================
// Types
// ============================================================================

export interface ResourceDiagnostic {
  type: "error" | "warning";
  path: string;
  message: string;
}

export interface DiscoveredResources {
  skillDirs: string[];
  promptDirs: string[];
  themeDirs: string[];
  contextFilePaths: string[];
  diagnostics: ResourceDiagnostic[];
}

// ============================================================================
// Loader
// ============================================================================

export function discoverResources(cwd: string): DiscoveredResources {
  const resolvedCwd = resolve(cwd);
  const globalDir = getPikoDir();
  const projectDir = resolve(resolvedCwd, ".piko");

  const result: DiscoveredResources = {
    skillDirs: [],
    promptDirs: [],
    themeDirs: [],
    contextFilePaths: [],
    diagnostics: [],
  };

  // Discover from global dir
  discoverDir(globalDir, result);

  // Discover from project dir (overrides global)
  if (projectDir !== globalDir) {
    discoverDir(projectDir, result);
  }

  return result;
}

function discoverDir(pikoDir: string, result: DiscoveredResources): void {
  if (!existsSync(pikoDir)) return;

  // Skills: .piko/skills/
  const skillsDir = join(pikoDir, "skills");
  if (isDir(skillsDir)) {
    result.skillDirs.push(skillsDir);
  }

  // Prompts: .piko/prompts/
  const promptsDir = join(pikoDir, "prompts");
  if (isDir(promptsDir)) {
    result.promptDirs.push(promptsDir);
  }

  // Themes: .piko/themes/
  const themesDir = join(pikoDir, "themes");
  if (isDir(themesDir)) {
    result.themeDirs.push(themesDir);
  }

  // Context files: AGENTS.md / CLAUDE.md in pikoDir root
  const contextCandidates = ["AGENTS.md", "AGENTS.MD", "CLAUDE.md", "CLAUDE.MD"];
  for (const name of contextCandidates) {
    const fp = join(pikoDir, name);
    if (existsSync(fp) && !result.contextFilePaths.includes(fp)) {
      result.contextFilePaths.push(fp);
    }
  }
}

function isDir(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}
