import * as fs from "node:fs/promises";
import { posix, resolve } from "node:path";
import {
  createToolTextOutput,
  DEFAULT_GREP_LIMIT,
  DEFAULT_IGNORED_DIRS,
  lineMatches,
  matchesGlob,
  resolvePathFromCwd,
  toPosixRelativePath,
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

async function readTextFile(absolutePath: string): Promise<string> {
  return fs.readFile(absolutePath, "utf-8");
}

interface GrepMatch {
  relativePath: string;
  lineNumber: number;
  lines: string[];
}

export async function grepTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const pattern = typeof args.pattern === "string" ? args.pattern : undefined;
  if (!pattern) throw new Error("grep requires a string pattern");
  const targetPath = typeof args.path === "string" && args.path.trim() ? args.path : ".";
  const glob = typeof args.glob === "string" && args.glob.trim() ? args.glob : undefined;
  const ignoreCase = args.ignoreCase === true;
  const literal = args.literal === true;
  const context = typeof args.context === "number" ? Math.max(0, Math.floor(args.context)) : 0;
  const limit =
    typeof args.limit === "number" ? Math.max(1, Math.floor(args.limit)) : DEFAULT_GREP_LIMIT;
  const absolutePath = resolvePathFromCwd(cwd, targetPath);
  const stats = await fs.stat(absolutePath);

  const files: WalkEntry[] = stats.isDirectory()
    ? (await walkDirectory(absolutePath)).filter((entry) => !entry.isDirectory)
    : [{ absolutePath, relativePath: posix.basename(targetPath), isDirectory: false }];

  const searchableFiles = glob
    ? files.filter((entry) => matchesGlob(entry.relativePath, glob))
    : files;

  const matches: GrepMatch[] = [];
  let matchLimitReached = false;

  for (const file of searchableFiles) {
    if (matches.length >= limit) {
      matchLimitReached = true;
      break;
    }
    let content: string;
    try {
      content = await readTextFile(file.absolutePath);
    } catch {
      continue;
    }
    if (content.includes("\u0000")) continue;

    const fileLines = content.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
    for (let lineIndex = 0; lineIndex < fileLines.length; lineIndex++) {
      if (matches.length >= limit) {
        matchLimitReached = true;
        break;
      }
      const line = fileLines[lineIndex] ?? "";
      if (!lineMatches(line, pattern, { ignoreCase, literal })) continue;
      const start = Math.max(0, lineIndex - context);
      const end = Math.min(fileLines.length - 1, lineIndex + context);
      const blockLines: string[] = [];
      for (let current = start; current <= end; current++) {
        const prefix =
          current === lineIndex
            ? `${file.relativePath}:${current + 1}: `
            : `${file.relativePath}-${current + 1}- `;
        blockLines.push(`${prefix}${fileLines[current] ?? ""}`);
      }
      matches.push({
        relativePath: file.relativePath,
        lineNumber: lineIndex + 1,
        lines: blockLines,
      });
    }
  }

  const flattened = matches.flatMap((match, index) =>
    index === 0 ? match.lines : ["", ...match.lines],
  );

  return createToolTextOutput(
    `grep ${literal ? "literal" : "pattern"} ${pattern} in ${targetPath}`,
    flattened.length > 0 ? flattened : ["No matches found"],
    matchLimitReached ? `Truncated to ${limit} matches` : undefined,
  );
}
