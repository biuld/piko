/**
 * Context files loader — loads AGENTS.md / CLAUDE.md from
 * project root, ancestor directories, and global config dir.
 */

import { getPikoDir } from "../session/index.js";
import { joinPath, resolvePath } from "../utils/bun-path.js";

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

async function loadFromDir(dir: string): Promise<ContextFile | null> {
  for (const filename of CANDIDATE_NAMES) {
    const filePath = joinPath(dir, filename);
    try {
      if (await Bun.file(filePath).exists()) {
        return { path: filePath, content: await Bun.file(filePath).text() };
      }
    } catch {
      // Ignore read failures
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
export async function loadContextFiles(options: LoadContextFilesOptions): Promise<ContextFile[]> {
  const resolvedCwd = resolvePath(options.cwd);

  const files: ContextFile[] = [];
  const seen = new Set<string>();

  // Global context file
  const globalFile = await loadFromDir(getPikoDir());
  if (globalFile) {
    files.push(globalFile);
    seen.add(globalFile.path);
  }

  // Ancestor context files (walk from cwd up to root)
  // Collected in reverse order, then reversed so deepest is last
  const ancestorFiles: ContextFile[] = [];

  let currentDir = resolvedCwd;
  const root = resolvePath("/");

  while (true) {
    const contextFile = await loadFromDir(currentDir);
    if (contextFile && !seen.has(contextFile.path)) {
      ancestorFiles.unshift(contextFile);
      seen.add(contextFile.path);
    }

    if (currentDir === root) break;

    const parentDir = resolvePath(currentDir, "..");
    if (parentDir === currentDir) break;
    currentDir = parentDir;
  }

  files.push(...ancestorFiles);

  return files;
}
