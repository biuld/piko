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

// ---- Callbacks interface ----

export interface HostToolCallbacks {
  askUser?: (question: string) => Promise<string>;
  requestApproval?: (
    action: string,
    details?: string,
  ) => Promise<{ approved: boolean; decision: string }>;
  requestUserInput?: (prompt: string, inputType?: string) => Promise<string>;
  openExternal?: (target: string) => Promise<void>;
}

// ---- Provider implementation ----

export class HostToolProvider implements ToolProvider {
  id = "host";
  source = "host" as const;

  private callbacks: HostToolCallbacks;

  constructor(callbacks: HostToolCallbacks = {}) {
    this.callbacks = callbacks;
  }

  async discover(_context: ToolDiscoveryContext): Promise<ToolDef[]> {
    return HOST_TOOLS;
  }

  async execute(call: ToolCall, _context: ToolExecutionContext): Promise<ToolExecResult> {
    try {
      switch (call.name) {
        case "ask_user": {
          if (!this.callbacks.askUser) {
            return { ok: false, error: { code: "no_handler", message: "ask_user not available" } };
          }
          const question =
            typeof call.arguments.question === "string" ? call.arguments.question : "";
          const answer = await this.callbacks.askUser(question);
          return { ok: true, value: answer };
        }

        case "request_approval": {
          if (!this.callbacks.requestApproval) {
            return {
              ok: false,
              error: { code: "no_handler", message: "request_approval not available" },
            };
          }
          const action = typeof call.arguments.action === "string" ? call.arguments.action : "";
          const details =
            typeof call.arguments.details === "string" ? call.arguments.details : undefined;
          const result = await this.callbacks.requestApproval(action, details);
          return { ok: true, value: result };
        }

        case "request_user_input": {
          if (!this.callbacks.requestUserInput) {
            return {
              ok: false,
              error: { code: "no_handler", message: "request_user_input not available" },
            };
          }
          const prompt = typeof call.arguments.prompt === "string" ? call.arguments.prompt : "";
          const inputType =
            typeof call.arguments.inputType === "string" ? call.arguments.inputType : undefined;
          const value = await this.callbacks.requestUserInput(prompt, inputType);
          return { ok: true, value };
        }

        case "open_external": {
          if (!this.callbacks.openExternal) {
            return {
              ok: false,
              error: { code: "no_handler", message: "open_external not available" },
            };
          }
          const target = typeof call.arguments.target === "string" ? call.arguments.target : "";
          await this.callbacks.openExternal(target);
          return { ok: true, value: `Opened: ${target}` };
        }

        default:
          return {
            ok: false,
            error: { code: "unknown_tool", message: `Unknown host tool: ${call.name}` },
          };
      }
    } catch (err) {
      return { ok: false, error: { code: "execution_error", message: fmtErr(err) } };
    }
  }
}

function fmtErr(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
