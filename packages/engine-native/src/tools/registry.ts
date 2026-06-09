import type { NativeToolRegistry } from "piko-engine-native";
import type { EngineTool, EngineToolSet } from "piko-engine-protocol";
import { applyPatchTool } from "./apply-patch/index.js";
import { shellTool } from "./shell.js";

// Re-export legacy tools for test/compat use
export { bashTool } from "./bash.js";
export { editTool } from "./edit.js";
export { findTool } from "./find.js";
export { grepTool } from "./grep.js";
export { lsTool } from "./ls.js";
export { readTool } from "./read.js";
export { writeTool } from "./write.js";

export interface BuiltinToolSet {
  definitions: EngineTool[];
  registry: NativeToolRegistry;
}

// ---- New default ToolSet (shell + apply_patch) ----

export const coreCodingToolSet: EngineToolSet = {
  id: "builtin:core-coding",
  name: "Core Coding",
  description: "Default coding tools: shell and apply_patch.",
  tools: [
    {
      name: "shell",
      description: "Execute a shell command in the workspace. Supports cat, rg, fd, ls, find, etc.",
      inputSchema: {
        type: "object",
        properties: {
          command: { type: "string", description: "Shell command to execute" },
          timeout: { type: "number", description: "Timeout in seconds" },
          cwd: { type: "string", description: "Working directory (relative to workspace root)" },
          login: { type: "boolean", description: "Use login shell (-l flag)" },
        },
        required: ["command"],
      },
      executor: { kind: "native", target: "shell" },
      executionMode: "sequential",
      exposure: "direct",
      capabilities: ["execute_process", "read_workspace", "write_workspace"],
      approval: "always",
    },
    {
      name: "apply_patch",
      description:
        "Apply a structured patch to files in the workspace. Use *** Begin Patch / *** End Patch grammar.",
      inputSchema: {
        type: "object",
        properties: {
          patch: { type: "string", description: "Patch content in Codex patch grammar" },
        },
        required: ["patch"],
      },
      executor: { kind: "native", target: "apply_patch" },
      executionMode: "sequential",
      exposure: "direct",
      capabilities: ["write_workspace"],
      approval: "always",
    },
  ],
  policy: {
    requiresWriteLock: true,
  },
};

function createCodingToolDefinitions(): EngineTool[] {
  return [
    {
      name: "shell",
      description: "Execute a shell command in the workspace. Supports cat, rg, fd, ls, find, etc.",
      inputSchema: {
        type: "object",
        properties: {
          command: { type: "string", description: "Shell command to execute" },
          timeout: { type: "number", description: "Timeout in seconds" },
          cwd: { type: "string", description: "Working directory (relative to workspace root)" },
          login: { type: "boolean", description: "Use login shell (-l flag)" },
        },
        required: ["command"],
      },
      executor: { kind: "native", target: "shell" },
      executionMode: "sequential",
      metadata: { requiresApproval: true },
    },
    {
      name: "apply_patch",
      description: "Apply a structured patch to files in the workspace.",
      inputSchema: {
        type: "object",
        properties: {
          patch: { type: "string", description: "Patch content in Codex patch grammar" },
        },
        required: ["patch"],
      },
      executor: { kind: "native", target: "apply_patch" },
      executionMode: "sequential",
      metadata: { requiresApproval: true },
    },
  ];
}

/** Default built-in tool set: shell + apply_patch. */
export function createBuiltinCodingToolSet(cwd: string = process.cwd()): BuiltinToolSet {
  return {
    definitions: createCodingToolDefinitions(),
    registry: {
      shell: (args) => shellTool(cwd, args),
      apply_patch: (args) => applyPatchTool(cwd, args),
    },
  };
}

// ---- Legacy file-operation toolset (for tests and compat) ----

import { bashTool } from "./bash.js";
import { editTool } from "./edit.js";
import { findTool } from "./find.js";
import { grepTool } from "./grep.js";
import { lsTool } from "./ls.js";
import { readTool } from "./read.js";
import { writeTool } from "./write.js";

function createLegacyToolDefinitions(): EngineTool[] {
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
      executionMode: "parallel",
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
      executionMode: "sequential",
      metadata: { requiresApproval: true },
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
              properties: { oldText: { type: "string" }, newText: { type: "string" } },
              required: ["oldText", "newText"],
            },
          },
        },
        required: ["path", "edits"],
      },
      executor: { kind: "native", target: "edit" },
      executionMode: "sequential",
      metadata: { requiresApproval: true },
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
      executionMode: "sequential",
      metadata: { requiresApproval: true },
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
      executionMode: "parallel",
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
      executionMode: "parallel",
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
      executionMode: "parallel",
    },
  ];
}

/**
 * Create a legacy file-operation toolset (read, write, edit, bash, grep, find, ls).
 * Intended for tests and transitional callers. Not used by default.
 */
export function createLegacyFileToolSet(cwd: string = process.cwd()): BuiltinToolSet {
  return {
    definitions: createLegacyToolDefinitions(),
    registry: {
      read: (args) => readTool(cwd, args),
      bash: (args) => bashTool(cwd, args),
      edit: (args) => editTool(cwd, args),
      write: (args) => writeTool(cwd, args),
      grep: (args) => grepTool(cwd, args),
      find: (args) => findTool(cwd, args),
      ls: (args) => lsTool(cwd, args),
    },
  };
}
