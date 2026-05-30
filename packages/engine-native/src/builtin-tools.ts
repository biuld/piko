import { spawn } from "node:child_process";
import * as fs from "node:fs/promises";
import { dirname, posix, relative, resolve } from "node:path";
import type { NativeToolRegistry } from "piko-engine-native";
import type { EngineTool } from "piko-engine-protocol";

interface BuiltinToolSet {
  definitions: EngineTool[];
  registry: NativeToolRegistry;
}

interface EditOperation {
  oldText: string;
  newText: string;
}

interface WalkEntry {
  absolutePath: string;
  relativePath: string;
  isDirectory: boolean;
}

interface GrepMatch {
  relativePath: string;
  lineNumber: number;
  lines: string[];
}

const DEFAULT_LS_LIMIT = 500;
const DEFAULT_FIND_LIMIT = 1000;
const DEFAULT_GREP_LIMIT = 100;
const DEFAULT_IGNORED_DIRS = new Set([".git", "node_modules"]);

function resolvePathFromCwd(cwd: string, filePath: string): string {
  return resolve(cwd, filePath);
}

function toPosixRelativePath(basePath: string, absolutePath: string): string {
  const raw = relative(basePath, absolutePath);
  if (!raw || raw === ".") return ".";
  return raw.split("\\").join("/");
}

function globToRegExp(pattern: string): RegExp {
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

function matchesGlob(pathValue: string, pattern: string): boolean {
  const normalizedPath = pathValue.split("\\").join("/");
  return globToRegExp(pattern).test(normalizedPath);
}

async function readTextFile(absolutePath: string): Promise<string> {
  return fs.readFile(absolutePath, "utf-8");
}

function countOccurrences(haystack: string, needle: string): number {
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

function createToolTextOutput(title: string, lines: string[], suffix?: string): string {
  const output = lines.length > 0 ? lines.join("\n") : "(no results)";
  return suffix ? `${title}\n${output}\n\n${suffix}` : `${title}\n${output}`;
}

function truncateLines(lines: string[], limit: number): { lines: string[]; truncated: boolean } {
  if (lines.length <= limit) return { lines, truncated: false };
  return {
    lines: lines.slice(0, limit),
    truncated: true,
  };
}

async function walkDirectory(rootPath: string): Promise<WalkEntry[]> {
  const entries: WalkEntry[] = [];

  async function walk(currentPath: string): Promise<void> {
    const dirEntries = await fs.readdir(currentPath, { withFileTypes: true });
    dirEntries.sort((left, right) => left.name.localeCompare(right.name));

    for (const dirEntry of dirEntries) {
      if (dirEntry.isDirectory() && DEFAULT_IGNORED_DIRS.has(dirEntry.name)) {
        continue;
      }

      const absolutePath = resolve(currentPath, dirEntry.name);
      const relativePath = toPosixRelativePath(rootPath, absolutePath);
      const isDirectory = dirEntry.isDirectory();
      entries.push({ absolutePath, relativePath, isDirectory });

      if (isDirectory) {
        await walk(absolutePath);
      }
    }
  }

  await walk(rootPath);
  return entries;
}

function lineMatches(
  line: string,
  pattern: string,
  options: {
    ignoreCase: boolean;
    literal: boolean;
  },
): boolean {
  if (options.literal) {
    if (options.ignoreCase) {
      return line.toLowerCase().includes(pattern.toLowerCase());
    }
    return line.includes(pattern);
  }

  const flags = options.ignoreCase ? "i" : "";
  const expression = new RegExp(pattern, flags);
  return expression.test(line);
}

async function executeReadTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const path = typeof args.path === "string" ? args.path : undefined;
  if (!path) throw new Error("read requires a string path");
  const offset = typeof args.offset === "number" ? Math.max(1, Math.floor(args.offset)) : 1;
  const limit = typeof args.limit === "number" ? Math.max(1, Math.floor(args.limit)) : undefined;
  const absolutePath = resolvePathFromCwd(cwd, path);
  const raw = await readTextFile(absolutePath);
  const lines = raw.split("\n");
  const selected = lines.slice(offset - 1, limit ? offset - 1 + limit : undefined);
  return {
    path,
    absolutePath,
    offset,
    limit: limit ?? null,
    lineCount: selected.length,
    content: selected.join("\n"),
  };
}

async function executeWriteTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const path = typeof args.path === "string" ? args.path : undefined;
  const content = typeof args.content === "string" ? args.content : undefined;
  if (!path) throw new Error("write requires a string path");
  if (content === undefined) throw new Error("write requires string content");
  const absolutePath = resolvePathFromCwd(cwd, path);
  await fs.mkdir(dirname(absolutePath), { recursive: true });
  await fs.writeFile(absolutePath, content, "utf-8");
  return {
    path,
    absolutePath,
    bytesWritten: Buffer.byteLength(content, "utf-8"),
    written: true,
  };
}

async function executeEditTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const path = typeof args.path === "string" ? args.path : undefined;
  if (!path) throw new Error("edit requires a string path");
  const edits = Array.isArray(args.edits) ? args.edits : undefined;
  if (!edits || edits.length === 0) {
    throw new Error("edit requires a non-empty edits array");
  }
  const normalizedEdits: EditOperation[] = edits.map((edit) => {
    if (!edit || typeof edit !== "object") {
      throw new Error("edit entries must be objects");
    }
    const oldText =
      typeof (edit as { oldText?: unknown }).oldText === "string"
        ? (edit as { oldText: string }).oldText
        : undefined;
    const newText =
      typeof (edit as { newText?: unknown }).newText === "string"
        ? (edit as { newText: string }).newText
        : undefined;
    if (oldText === undefined || newText === undefined) {
      throw new Error("each edit requires oldText and newText strings");
    }
    return { oldText, newText };
  });

  const absolutePath = resolvePathFromCwd(cwd, path);
  let content = await readTextFile(absolutePath);
  for (const edit of normalizedEdits) {
    const matches = countOccurrences(content, edit.oldText);
    if (matches === 0) {
      throw new Error(`edit target not found in ${path}`);
    }
    if (matches > 1) {
      throw new Error(`edit target is ambiguous in ${path}`);
    }
    content = content.replace(edit.oldText, edit.newText);
  }
  await fs.writeFile(absolutePath, content, "utf-8");
  return {
    path,
    absolutePath,
    editsApplied: normalizedEdits.length,
    patched: true,
  };
}

async function executeBashTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const command = typeof args.command === "string" ? args.command : undefined;
  if (!command) throw new Error("bash requires a string command");
  const timeoutSeconds =
    typeof args.timeout === "number" && args.timeout > 0 ? args.timeout : undefined;
  const shell = process.env.SHELL || "/bin/sh";
  const shellArgs =
    shell.includes("zsh") || shell.includes("bash") ? ["-lc", command] : ["-c", command];

  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(shell, shellArgs, {
      cwd,
      env: process.env,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    let timeoutHandle: NodeJS.Timeout | undefined;

    if (timeoutSeconds) {
      timeoutHandle = setTimeout(() => {
        child.kill("SIGTERM");
      }, timeoutSeconds * 1000);
    }

    child.stdout?.on("data", (chunk: Buffer | string) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk: Buffer | string) => {
      stderr += chunk.toString();
    });
    child.on("error", (error) => {
      if (timeoutHandle) clearTimeout(timeoutHandle);
      rejectPromise(error);
    });
    child.on("close", (code, signal) => {
      if (timeoutHandle) clearTimeout(timeoutHandle);
      resolvePromise({
        command,
        exitCode: code,
        signal: signal ?? null,
        stdout,
        stderr,
        output: `${stdout}${stderr}`.trim(),
      });
    });
  });
}

async function executeLsTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const targetPath = typeof args.path === "string" && args.path.trim() ? args.path : ".";
  const limit =
    typeof args.limit === "number" ? Math.max(1, Math.floor(args.limit)) : DEFAULT_LS_LIMIT;
  const absolutePath = resolvePathFromCwd(cwd, targetPath);
  const stats = await fs.stat(absolutePath);
  if (!stats.isDirectory()) {
    throw new Error(`ls requires a directory path: ${targetPath}`);
  }

  const dirEntries = await fs.readdir(absolutePath, { withFileTypes: true });
  dirEntries.sort((left, right) => left.name.localeCompare(right.name));

  const formattedEntries = dirEntries.map((entry) =>
    entry.isDirectory() ? `${entry.name}/` : entry.name,
  );
  const truncated = truncateLines(formattedEntries, limit);

  return createToolTextOutput(
    `ls ${targetPath}`,
    truncated.lines,
    truncated.truncated ? `Truncated to ${limit} entries` : undefined,
  );
}

async function executeFindTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
  const pattern = typeof args.pattern === "string" ? args.pattern : undefined;
  if (!pattern) throw new Error("find requires a string pattern");
  const targetPath = typeof args.path === "string" && args.path.trim() ? args.path : ".";
  const limit =
    typeof args.limit === "number" ? Math.max(1, Math.floor(args.limit)) : DEFAULT_FIND_LIMIT;
  const absolutePath = resolvePathFromCwd(cwd, targetPath);
  const stats = await fs.stat(absolutePath);
  if (!stats.isDirectory()) {
    throw new Error(`find requires a directory path: ${targetPath}`);
  }

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

async function executeGrepTool(cwd: string, args: Record<string, unknown>): Promise<unknown> {
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
    : [
        {
          absolutePath,
          relativePath: posix.basename(targetPath),
          isDirectory: false,
        },
      ];

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

    if (content.includes("\u0000")) {
      continue;
    }

    const fileLines = content.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");

    for (let lineIndex = 0; lineIndex < fileLines.length; lineIndex++) {
      if (matches.length >= limit) {
        matchLimitReached = true;
        break;
      }
      const line = fileLines[lineIndex] ?? "";
      if (!lineMatches(line, pattern, { ignoreCase, literal })) {
        continue;
      }

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

function createCodingToolDefinitions(): EngineTool[] {
  return [
    {
      name: "read",
      description: "Read the contents of a file.",
      inputSchema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to the file to read" },
          offset: { type: "number", description: "Line number to start reading from (1-indexed)" },
          limit: { type: "number", description: "Maximum number of lines to read" },
        },
        required: ["path"],
      },
      executor: { kind: "native", target: "read" },
    },
    {
      name: "bash",
      description: "Execute a shell command in the current working directory.",
      inputSchema: {
        type: "object",
        properties: {
          command: { type: "string", description: "Shell command to execute" },
          timeout: { type: "number", description: "Timeout in seconds" },
        },
        required: ["command"],
      },
      executor: { kind: "native", target: "bash" },
    },
    {
      name: "edit",
      description: "Apply one or more exact text replacements to an existing file.",
      inputSchema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to the file to edit" },
          edits: {
            type: "array",
            items: {
              type: "object",
              properties: {
                oldText: { type: "string" },
                newText: { type: "string" },
              },
              required: ["oldText", "newText"],
            },
          },
        },
        required: ["path", "edits"],
      },
      executor: { kind: "native", target: "edit" },
    },
    {
      name: "write",
      description: "Write content to a file, creating parent directories when needed.",
      inputSchema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to the file to write" },
          content: { type: "string", description: "File contents" },
        },
        required: ["path", "content"],
      },
      executor: { kind: "native", target: "write" },
    },
    {
      name: "grep",
      description: "Search file contents for a pattern. Useful for codebase exploration.",
      inputSchema: {
        type: "object",
        properties: {
          pattern: { type: "string", description: "Regex or literal search pattern" },
          path: { type: "string", description: "Directory or file to search" },
          glob: {
            type: "string",
            description: "Optional glob filter like '*.ts' or 'src/**/*.ts'",
          },
          ignoreCase: { type: "boolean", description: "Case-insensitive search" },
          literal: { type: "boolean", description: "Treat pattern as a literal string" },
          context: { type: "number", description: "Context lines before and after each match" },
          limit: { type: "number", description: "Maximum number of matches to return" },
        },
        required: ["pattern"],
      },
      executor: { kind: "native", target: "grep" },
    },
    {
      name: "find",
      description: "Find files by glob pattern under the current workspace.",
      inputSchema: {
        type: "object",
        properties: {
          pattern: { type: "string", description: "Glob pattern like '*.ts' or 'src/**/*.ts'" },
          path: { type: "string", description: "Directory to search in" },
          limit: { type: "number", description: "Maximum number of results to return" },
        },
        required: ["pattern"],
      },
      executor: { kind: "native", target: "find" },
    },
    {
      name: "ls",
      description: "List directory contents, including dotfiles.",
      inputSchema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Directory to list" },
          limit: { type: "number", description: "Maximum number of directory entries to return" },
        },
      },
      executor: { kind: "native", target: "ls" },
    },
  ];
}

export function createBuiltinCodingToolSet(cwd: string = process.cwd()): BuiltinToolSet {
  const registry: NativeToolRegistry = {
    read: (args) => executeReadTool(cwd, args),
    bash: (args) => executeBashTool(cwd, args),
    edit: (args) => executeEditTool(cwd, args),
    write: (args) => executeWriteTool(cwd, args),
    grep: (args) => executeGrepTool(cwd, args),
    find: (args) => executeFindTool(cwd, args),
    ls: (args) => executeLsTool(cwd, args),
  };

  return {
    definitions: createCodingToolDefinitions(),
    registry,
  };
}
