import { relative, resolve } from "node:path";

export interface EditOperation {
  oldText: string;
  newText: string;
}

export interface WalkEntry {
  absolutePath: string;
  relativePath: string;
  isDirectory: boolean;
}

export interface GrepMatch {
  relativePath: string;
  lineNumber: number;
  lines: string[];
}

export const DEFAULT_LS_LIMIT = 500;
export const DEFAULT_FIND_LIMIT = 1000;
export const DEFAULT_GREP_LIMIT = 100;
export const DEFAULT_IGNORED_DIRS = new Set([".git", "node_modules"]);

export function resolvePathFromCwd(cwd: string, filePath: string): string {
  return resolve(cwd, filePath);
}

export function toPosixRelativePath(basePath: string, absolutePath: string): string {
  const raw = relative(basePath, absolutePath);
  if (!raw || raw === ".") return ".";
  return raw.split("\\").join("/");
}

export function globToRegExp(pattern: string): RegExp {
  const normalized = pattern.split("\\").join("/");
  let source = "^";
  for (let index = 0; index < normalized.length; index++) {
    const char = normalized[index];
    const next = normalized[index + 1];
    const nextNext = normalized[index + 2];
    if (char === "*" && next === "*" && nextNext === "/") {
      source += "(?:.*/)?";
      index += 2;
      continue;
    }
    if (char === "*" && next === "*") {
      source += ".*";
      index += 1;
      continue;
    }
    if (char === "*") {
      source += "[^/]*";
      continue;
    }
    if (char === "?") {
      source += "[^/]";
      continue;
    }
    if ("\\^$+?.()|{}[]".includes(char)) {
      source += `\\${char}`;
      continue;
    }
    source += char;
  }
  source += "$";
  return new RegExp(source);
}

export function matchesGlob(pathValue: string, pattern: string): boolean {
  return globToRegExp(pattern).test(pathValue.split("\\").join("/"));
}

export function countOccurrences(haystack: string, needle: string): number {
  if (!needle) return 0;
  let count = 0;
  let start = 0;
  while (true) {
    const index = haystack.indexOf(needle, start);
    if (index === -1) return count;
    count++;
    start = index + needle.length;
  }
}

export function truncateLines(
  lines: string[],
  limit: number,
): { lines: string[]; truncated: boolean } {
  if (lines.length <= limit) return { lines, truncated: false };
  return { lines: lines.slice(0, limit), truncated: true };
}

export function createToolTextOutput(title: string, lines: string[], suffix?: string): string {
  const output = lines.length > 0 ? lines.join("\n") : "(no results)";
  return suffix ? `${title}\n${output}\n\n${suffix}` : `${title}\n${output}`;
}

export function lineMatches(
  line: string,
  pattern: string,
  options: { ignoreCase: boolean; literal: boolean },
): boolean {
  if (options.literal) {
    return options.ignoreCase
      ? line.toLowerCase().includes(pattern.toLowerCase())
      : line.includes(pattern);
  }
  return new RegExp(pattern, options.ignoreCase ? "i" : "").test(line);
}
