import type { NativeToolRegistry } from "piko-engine-native";
import type { EngineTool } from "piko-engine-protocol";
import { bashTool } from "./bash.js";
import { editTool } from "./edit.js";
import { findTool } from "./find.js";
import { grepTool } from "./grep.js";
import { lsTool } from "./ls.js";
import { readTool } from "./read.js";
import { writeTool } from "./write.js";

export interface BuiltinToolSet {
  definitions: EngineTool[];
  registry: NativeToolRegistry;
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
  return {
    definitions: createCodingToolDefinitions(),
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
