// ---- HostToolProvider — Host/TUI bridge tools ----

import type {
  ToolCall,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
} from "piko-orchestrator-protocol";

// ---- Tool definitions ----

export type HostToolHandler = (
  args: Record<string, unknown>,
  context: ToolExecutionContext,
  call: ToolCall,
) => Promise<unknown>;

const HOST_TOOLS: ToolDef[] = [
  {
    name: "ask_user",
    description: "Ask the user a direct question through the host/TUI.",
    inputSchema: {
      type: "object",
      properties: {
        question: { type: "string", description: "Question to ask the user" },
      },
      required: ["question"],
    },
    executor: { kind: "host", target: "ask_user" },
    capabilities: ["request_user_input"],
  },
  {
    name: "request_approval",
    description: "Request user approval for an action not tied to a ToolActor policy.",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", description: "Description of the action to approve" },
        details: { type: "string", description: "Additional context" },
      },
      required: ["action"],
    },
    executor: { kind: "host", target: "request_approval" },
    capabilities: ["request_user_input"],
  },
  {
    name: "request_user_input",
    description: "Request arbitrary user input through the host/TUI.",
    inputSchema: {
      type: "object",
      properties: {
        prompt: { type: "string", description: "Prompt for the user" },
        inputType: {
          type: "string",
          enum: ["text", "confirm", "choice"],
          description: "Type of input to request",
        },
      },
      required: ["prompt"],
    },
    executor: { kind: "host", target: "request_user_input" },
    capabilities: ["request_user_input"],
  },
  {
    name: "open_external",
    description: "Ask the host to open a URL, file, or application.",
    inputSchema: {
      type: "object",
      properties: {
        target: { type: "string", description: "URL, file path, or app name to open" },
      },
      required: ["target"],
    },
    executor: { kind: "host", target: "open_external" },
    capabilities: ["network"],
    approval: "always",
  },
];

// ---- Provider implementation ----

export class HostToolProvider implements ToolProvider {
  id = "host";
  source = "host" as const;

  private handlers: Record<string, HostToolHandler>;

  constructor(handlers: Record<string, HostToolHandler> = {}) {
    this.handlers = handlers;
  }

  /** Register or replace a handler for a host tool. */
  setHandler(toolName: string, handler: HostToolHandler): void {
    this.handlers[toolName] = handler;
  }

  async discover(_context: ToolDiscoveryContext): Promise<ToolDef[]> {
    return HOST_TOOLS;
  }

  async execute(call: ToolCall, _context: ToolExecutionContext): Promise<ToolExecResult> {
    const handler = this.handlers[call.name];
    if (!handler) {
      return {
        ok: false,
        error: {
          code: "no_handler",
          message: `No host handler registered for tool: ${call.name}`,
        },
      };
    }

    try {
      const value = await handler(call.arguments, _context, call);
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
