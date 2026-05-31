/**
 * Context files loader — loads AGENTS.md / CLAUDE.md from
 * project root, ancestor directories, and global config dir.
 */

import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { getPikoDir } from "../session/index.js";

// ============================================================================
// Types
// ============================================================================

export interface ContextFile {
  path: string;
  content: string;
}

// ============================================================================
// Loader
// ============================================================================

const CANDIDATE_NAMES = ["AGENTS.md", "AGENTS.MD", "CLAUDE.md", "CLAUDE.MD"];

function loadFromDir(dir: string): ContextFile | null {
  for (const filename of CANDIDATE_NAMES) {
    const filePath = join(dir, filename);
    if (existsSync(filePath)) {
      try {
        return { path: filePath, content: readFileSync(filePath, "utf-8") };
      } catch {
        // Ignore read failures
      }
    }
  }
  return null;
}

export interface LoadContextFilesOptions {
  /** Working directory. */
  cwd: string;
}

/**
 * Load project context files from:
 * 1. Global: ~/.piko/AGENTS.md
 * 2. Ancestors: walking from cwd up to root, collecting AGENTS.md files
 *
 * Returns files ordered from most general (global) to most specific (project).
 */
export function loadContextFiles(options: LoadContextFilesOptions): ContextFile[] {
  const resolvedCwd = resolve(options.cwd);

  const files: ContextFile[] = [];
  const seen = new Set<string>();

  // Global context file
  const globalFile = loadFromDir(getPikoDir());
  if (globalFile) {
    files.push(globalFile);
    seen.add(globalFile.path);
  }

  // Ancestor context files (walk from cwd up to root)
  // Collected in reverse order, then reversed so deepest is last
  const ancestorFiles: ContextFile[] = [];

  let currentDir = resolvedCwd;
  const root = resolve("/");

  while (true) {
    const contextFile = loadFromDir(currentDir);
    if (contextFile && !seen.has(contextFile.path)) {
      ancestorFiles.unshift(contextFile);
      seen.add(contextFile.path);
    }

    if (currentDir === root) break;

    const parentDir = resolve(currentDir, "..");
    if (parentDir === currentDir) break;
    currentDir = parentDir;
  }

  files.push(...ancestorFiles);

  return files;
}
