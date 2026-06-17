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

import { getPikoDir } from "./session/index.js";
import { joinPath, resolvePath } from "./utils/bun-path.js";

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
  const resolvedCwd = resolvePath(cwd);
  const globalDir = getPikoDir();
  const projectDir = resolvePath(resolvedCwd, ".piko");

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
  if (!isDir(pikoDir)) return;

  // Skills: .piko/skills/
  const skillsDir = joinPath(pikoDir, "skills");
  if (isDir(skillsDir)) {
    result.skillDirs.push(skillsDir);
  }

  // Prompts: .piko/prompts/
  const promptsDir = joinPath(pikoDir, "prompts");
  if (isDir(promptsDir)) {
    result.promptDirs.push(promptsDir);
  }

  // Themes: .piko/themes/
  const themesDir = joinPath(pikoDir, "themes");
  if (isDir(themesDir)) {
    result.themeDirs.push(themesDir);
  }

  // Context files: AGENTS.md / CLAUDE.md in pikoDir root
  const contextCandidates = ["AGENTS.md", "AGENTS.MD", "CLAUDE.md", "CLAUDE.MD"];
  for (const name of contextCandidates) {
    const fp = joinPath(pikoDir, name);
    if (hasFile(pikoDir, name) && !result.contextFilePaths.includes(fp)) {
      result.contextFilePaths.push(fp);
    }
  }
}

function isDir(path: string): boolean {
  try {
    const glob = new Bun.Glob("*");
    for (const _entry of glob.scanSync({ cwd: path, onlyFiles: false, dot: true })) {
      break;
    }
    return true;
  } catch {
    return false;
  }
}

function hasFile(cwd: string, name: string): boolean {
  try {
    for (const _entry of new Bun.Glob(name).scanSync({ cwd, onlyFiles: true, dot: true })) {
      return true;
    }
  } catch {
    return false;
  }
  return false;
}
