// ---- NativeToolProvider — wraps engine-native tool execution ----

import type {
  ToolCall,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
} from "piko-protocol";

// ---- Tool definitions for engine-native tools ----

const ENGINE_TOOLS: ToolDef[] = [
  {
    name: "read",
    description:
      "Read the contents of a file. Supports text files and images (jpg, png, gif, webp). Images are sent as attachments. For text files, output is truncated to 2000 lines or 50KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string", description: "Path to the file to read (relative or absolute)" },
        offset: { type: "number", description: "Line number to start reading from (1-indexed)" },
        limit: { type: "number", description: "Maximum number of lines to read" },
      },
      required: ["path"],
    },
    executor: { kind: "native", target: "read" },
    capabilities: ["read_workspace"],
  },
  {
    name: "bash",
    description:
      "Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last 2000 lines or 50KB (whichever is hit first). If truncated, full output is saved to a temp file. Optionally provide a timeout in seconds.",
    inputSchema: {
      type: "object",
      properties: {
        command: { type: "string", description: "Bash command to execute" },
        timeout: {
          type: "number",
          description: "Timeout in seconds (optional, no default timeout)",
        },
      },
      required: ["command"],
    },
    executor: { kind: "native", target: "shell" },
    capabilities: ["execute_process", "read_workspace"],
    approval: "on_request",
  },
  {
    name: "edit",
    description:
      "Edit a single file using exact text replacement. Every edits[].oldText must match a unique, non-overlapping region of the original file. If two changes affect the same block or nearby lines, merge them into one edit instead of emitting overlapping edits. Do not include large unchanged regions just to connect distant changes.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string", description: "Path to the file to edit (relative or absolute)" },
        edits: {
          type: "array",
          items: {
            type: "object",
            properties: {
              oldText: { type: "string", description: "Exact text for one targeted replacement" },
              newText: { type: "string", description: "Replacement text for this targeted edit" },
            },
            required: ["oldText", "newText"],
          },
          description: "One or more targeted replacements",
        },
      },
      required: ["path", "edits"],
    },
    executor: { kind: "native", target: "apply_patch" },
    capabilities: ["write_workspace"],
    approval: "on_request",
  },
  {
    name: "write",
    description:
      "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string", description: "Path to the file to write (relative or absolute)" },
        content: { type: "string", description: "Content to write to the file" },
      },
      required: ["path", "content"],
    },
    executor: { kind: "native", target: "write" },
    capabilities: ["write_workspace"],
  },
  {
    name: "grep",
    description: "Search file contents using ripgrep or grep.",
    inputSchema: {
      type: "object",
      properties: {
        pattern: { type: "string", description: "Search pattern" },
        path: { type: "string", description: "Directory or file path to search" },
      },
      required: ["pattern"],
    },
    executor: { kind: "native", target: "grep" },
    capabilities: ["read_workspace"],
  },
  {
    name: "find",
    description: "Find files by path/name pattern.",
    inputSchema: {
      type: "object",
      properties: {
        pattern: { type: "string", description: "File name pattern" },
        path: { type: "string", description: "Starting directory" },
      },
      required: ["pattern"],
    },
    executor: { kind: "native", target: "find" },
    capabilities: ["read_workspace"],
  },
  {
    name: "ls",
    description: "List directory contents.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string", description: "Directory path (defaults to cwd)" },
      },
    },
    executor: { kind: "native", target: "ls" },
    capabilities: ["read_workspace"],
  },
  {
    name: "view_image",
    description: "Inspect a local image file.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string", description: "Path to the image file" },
      },
      required: ["path"],
    },
    executor: { kind: "native", target: "view_image" },
    capabilities: ["view_image"],
  },
];

// ---- Provider implementation ----

/**
 * NativeToolProvider wraps native tool execution using the NativeToolRegistry.
 * The actual tool functions are injected by the Host at construction time.
 */
export class NativeToolProvider implements ToolProvider {
  id = "engine";
  source = "engine" as const;

  private registry: Record<string, (args: Record<string, unknown>) => Promise<unknown>>;

  constructor(registry: Record<string, (args: Record<string, unknown>) => Promise<unknown>>) {
    this.registry = registry;
  }

  async discover(_context: ToolDiscoveryContext): Promise<ToolDef[]> {
    return ENGINE_TOOLS;
  }

  async execute(call: ToolCall, _context: ToolExecutionContext): Promise<ToolExecResult> {
    // Handle write specially (it maps to apply_patch internally)
    const executorKey = call.name === "write" ? "write" : call.name;

    const executor = this.registry[executorKey];
    if (!executor) {
      // Try fallback mappings
      const fallbackExecutor =
        call.name === "bash"
          ? this.registry.shell
          : call.name === "edit"
            ? this.registry.apply_patch
            : undefined;

      if (fallbackExecutor) {
        try {
          const value = await fallbackExecutor(call.arguments);
          return { ok: true, value };
        } catch (err) {
          return {
            ok: false,
            error: {
              code: "execution_error",
              message: err instanceof Error ? err.message : String(err),
            },
          };
        }
      }

      return {
        ok: false,
        error: { code: "not_found", message: `No executor for tool: ${call.name}` },
      };
    }

    try {
      const value = await executor(call.arguments);
      return { ok: true, value };
    } catch (err) {
      return {
        ok: false,
        error: {
          code: "execution_error",
          message: err instanceof Error ? err.message : String(err),
        },
      };
    }
  }
}
