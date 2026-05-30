import * as fs from "node:fs/promises";
import { resolve } from "node:path";
import {
  createToolTextOutput,
  DEFAULT_FIND_LIMIT,
  DEFAULT_IGNORED_DIRS,
  matchesGlob,
  resolvePathFromCwd,
  toPosixRelativePath,
  truncateLines,
  type WalkEntry,
} from "./utils.js";

async function walkDirectory(rootPath: string): Promise<WalkEntry[]> {
  const entries: WalkEntry[] = [];
  async function walk(currentPath: string): Promise<void> {
    const dirEntries = await fs.readdir(currentPath, { withFileTypes: true });
    dirEntries.sort((left, right) => left.name.localeCompare(right.name));
    for (const dirEntry of dirEntries) {
      if (dirEntry.isDirectory() && DEFAULT_IGNORED_DIRS.has(dirEntry.name)) continue;
      const absolutePath = resolve(currentPath, dirEntry.name);
      const relativePath = toPosixRelativePath(rootPath, absolutePath);
      const isDirectory = dirEntry.isDirectory();
      entries.push({ absolutePath, relativePath, isDirectory });
      if (isDirectory) await walk(absolutePath);
    }
  }
  await walk(rootPath);
  return entries;
}

export async function findTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const pattern = typeof args.pattern === "string" ? args.pattern : undefined;
  if (!pattern) throw new Error("find requires a string pattern");
  const targetPath = typeof args.path === "string" && args.path.trim() ? args.path : ".";
  const limit =
    typeof args.limit === "number" ? Math.max(1, Math.floor(args.limit)) : DEFAULT_FIND_LIMIT;
  const absolutePath = resolvePathFromCwd(cwd, targetPath);
  const stats = await fs.stat(absolutePath);
  if (!stats.isDirectory()) throw new Error(`find requires a directory path: ${targetPath}`);

  const walked = await walkDirectory(absolutePath);
  const matches = walked
    .filter((entry) => matchesGlob(entry.relativePath, pattern))
    .map((entry) => (entry.isDirectory ? `${entry.relativePath}/` : entry.relativePath));
  const truncated = truncateLines(matches, limit);

  return createToolTextOutput(
    `find ${pattern} in ${targetPath}`,
    truncated.lines.length > 0 ? truncated.lines : ["No files found"],
    truncated.truncated ? `Truncated to ${limit} results` : undefined,
  );
}
