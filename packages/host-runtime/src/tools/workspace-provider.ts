// ---- WorkspaceToolProvider — filesystem, process, and image tools ----

import type {
  ToolCall,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
} from "piko-orchestrator-protocol";
import type { ExecutionEnv } from "../session/exec-env.js";

// ---- Tool definitions ----

const WORKSPACE_TOOLS: ToolDef[] = [
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

export class WorkspaceToolProvider implements ToolProvider {
  id = "workspace";
  source = "workspace" as const;

  private env: ExecutionEnv;

  constructor(env: ExecutionEnv) {
    this.env = env;
  }

  async discover(_context: ToolDiscoveryContext): Promise<ToolDef[]> {
    return [...WORKSPACE_TOOLS];
  }

  async execute(call: ToolCall, _context: ToolExecutionContext): Promise<ToolExecResult> {
    return this.executeBuiltin(call);
  }

  private async executeBuiltin(call: ToolCall): Promise<ToolExecResult> {
    try {
      switch (call.name) {
        case "read":
          return this.handleRead(call);
        case "bash":
          return this.handleBash(call);
        case "edit":
          return this.handleEdit(call);
        case "write":
          return this.handleWrite(call);
        case "grep":
          return this.handleGrep(call);
        case "find":
          return this.handleFind(call);
        case "ls":
          return this.handleLs(call);
        case "view_image":
          return this.handleViewImage(call);
        default:
          return { ok: false, error: { code: "not_found", message: `Unknown tool: ${call.name}` } };
      }
    } catch (err) {
      return { ok: false, error: { code: "execution_error", message: fmtErr(err) } };
    }
  }

  // ---- Built-in tool implementations ----

  private async handleRead(call: ToolCall): Promise<ToolExecResult> {
    const path = asString(call.arguments.path);
    const offset = asNumber(call.arguments.offset);
    const limit = asNumber(call.arguments.limit);

    const resolved = await this.env.canonicalPath(path);
    if (!resolved.ok)
      return { ok: false, error: { code: "not_found", message: resolved.error.message } };

    // Try reading as text first
    const textResult = await this.env.readTextFile(resolved.value);
    if (textResult.ok) {
      let text = textResult.value;
      if (text.length > 50 * 1024) {
        text = text.slice(0, 50 * 1024);
      }
      if (offset || limit) {
        const lines = text.split("\n");
        const start = (offset ?? 1) - 1;
        const end = limit ? start + limit : Math.min(start + 2000, lines.length);
        text = lines.slice(start, end).join("\n");
        if (lines.length > end) {
          text += `\n\n[Truncated: ${lines.length - end} more lines]`;
        }
      } else if (text.split("\n").length > 2000) {
        const lines = text.split("\n");
        text = lines.slice(0, 2000).join("\n");
        text += `\n\n[Truncated: ${lines.length - 2000} more lines]`;
      }
      return { ok: true, value: text };
    }

    // Try as binary (image)
    const binResult = await this.env.readBinaryFile(resolved.value);
    if (binResult.ok) {
      return { ok: true, value: binResult.value };
    }

    return { ok: false, error: { code: "not_found", message: `Cannot read: ${path}` } };
  }

  private async handleBash(call: ToolCall): Promise<ToolExecResult> {
    const command = asString(call.arguments.command);
    const timeout = asNumber(call.arguments.timeout);

    const result = await this.env.exec(command, { timeout });
    if (!result.ok) {
      return { ok: false, error: { code: "execution_error", message: result.error.message } };
    }

    let output = result.value.stdout;
    if (result.value.stderr) {
      output += `\n[stderr]\n${result.value.stderr}`;
    }
    if (result.value.exitCode !== 0) {
      output += `\n[exit code: ${result.value.exitCode}]`;
    }

    // Truncate if needed
    const lines = output.split("\n");
    if (lines.length > 2000 || output.length > 50 * 1024) {
      output = lines.slice(0, 2000).join("\n");
      if (output.length > 50 * 1024) {
        output = output.slice(0, 50 * 1024);
      }
    }

    return { ok: true, value: output };
  }

  private async handleEdit(call: ToolCall): Promise<ToolExecResult> {
    const path = asString(call.arguments.path);
    const edits = call.arguments.edits as Array<{ oldText: string; newText: string }> | undefined;

    if (!edits || !Array.isArray(edits) || edits.length === 0) {
      return { ok: false, error: { code: "invalid_args", message: "edits array is required" } };
    }

    const resolved = await this.env.canonicalPath(path);
    if (!resolved.ok) {
      return { ok: false, error: { code: "not_found", message: resolved.error.message } };
    }

    const readResult = await this.env.readTextFile(resolved.value);
    if (!readResult.ok) {
      return { ok: false, error: { code: "not_found", message: readResult.error.message } };
    }

    let content = readResult.value;
    for (const edit of edits) {
      if (!content.includes(edit.oldText)) {
        return {
          ok: false,
          error: {
            code: "invalid_args",
            message: `oldText not found in file: "${edit.oldText.slice(0, 80)}"`,
          },
        };
      }
      content = content.replace(edit.oldText, edit.newText);
    }

    const writeResult = await this.env.writeFile(resolved.value, content);
    if (!writeResult.ok) {
      return { ok: false, error: { code: "execution_error", message: writeResult.error.message } };
    }

    return { ok: true, value: `Applied ${edits.length} edit(s) to ${path}` };
  }

  private async handleWrite(call: ToolCall): Promise<ToolExecResult> {
    const path = asString(call.arguments.path);
    const content = asString(call.arguments.content);

    const resolved = await this.env.canonicalPath(path);
    let targetPath: string;
    if (resolved.ok) {
      targetPath = resolved.value;
    } else {
      targetPath = path;
    }

    // Ensure parent directories exist
    const parentDir = targetPath.split("/").slice(0, -1).join("/") || "/";
    const dirResult = await this.env.createDir(parentDir, { recursive: true });
    if (!dirResult.ok) {
      // Non-fatal: directory may already exist
    }

    const result = await this.env.writeFile(targetPath, content);
    if (!result.ok) {
      return { ok: false, error: { code: "execution_error", message: result.error.message } };
    }

    return { ok: true, value: `Wrote ${content.length} bytes to ${path}` };
  }

  private async handleGrep(call: ToolCall): Promise<ToolExecResult> {
    const pattern = asString(call.arguments.pattern);
    const path = typeof call.arguments.path === "string" ? call.arguments.path : ".";

    const result = await this.env.exec(`rg --no-heading --line-number "${pattern}" ${path}`, {});
    if (!result.ok) {
      // rg not found or failed — fall back to grep
      const grepResult = await this.env.exec(`grep -rn "${pattern}" ${path}`, {});
      if (!grepResult.ok) {
        return { ok: true, value: "" };
      }
      return { ok: true, value: grepResult.value.stdout };
    }
    return { ok: true, value: result.value.stdout };
  }

  private async handleFind(call: ToolCall): Promise<ToolExecResult> {
    const pattern = asString(call.arguments.pattern);
    const path = typeof call.arguments.path === "string" ? call.arguments.path : ".";

    const result = await this.env.exec(`find ${path} -name "${pattern}"`, {});
    if (!result.ok) {
      return { ok: false, error: { code: "execution_error", message: result.error.message } };
    }
    return { ok: true, value: result.value.stdout };
  }

  private async handleLs(call: ToolCall): Promise<ToolExecResult> {
    const path = typeof call.arguments.path === "string" ? call.arguments.path : ".";

    const resolved = await this.env.canonicalPath(path);
    const targetPath = resolved.ok ? resolved.value : path;

    const result = await this.env.listDir(targetPath);
    if (!result.ok) {
      return { ok: false, error: { code: "not_found", message: result.error.message } };
    }

    const listing = result.value
      .map((f) => `${f.kind === "directory" ? "d" : "-"} ${f.name} (${f.size} bytes)`)
      .join("\n");

    return { ok: true, value: listing || "(empty directory)" };
  }

  private async handleViewImage(call: ToolCall): Promise<ToolExecResult> {
    const path = asString(call.arguments.path);

    const resolved = await this.env.canonicalPath(path);
    if (!resolved.ok) {
      return { ok: false, error: { code: "not_found", message: resolved.error.message } };
    }

    const result = await this.env.readBinaryFile(resolved.value);
    if (!result.ok) {
      return { ok: false, error: { code: "not_found", message: result.error.message } };
    }

    // Return binary data — the TUI renderer handles image display
    return { ok: true, value: result.value };
  }
}

// ---- Helpers ----

function asString(v: unknown): string {
  return typeof v === "string" ? v : "";
}

function asNumber(v: unknown): number | undefined {
  return typeof v === "number" ? v : undefined;
}

function fmtErr(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
