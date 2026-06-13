// ---- OrchToolProvider — orchestrator control tools ----
// Lives in host-runtime; depends on the Orchestrator facade (not ActorSystem).

import type { Orchestrator } from "piko-orchestrator";
import type {
  ToolCall,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
} from "piko-orchestrator-protocol";

// ---- Tool definitions ----

const ORCH_TOOLS: ToolDef[] = [
  {
    name: "delegate_to_agent",
    description:
      "Delegate a task to a subagent in 'call' (blocking) or 'detach' (fire-and-forget with later join) mode.",
    inputSchema: {
      type: "object",
      properties: {
        agentId: {
          type: "string",
          description: "Target agent ID",
        },
        prompt: {
          type: "string",
          description: "Task description for the subagent",
        },
        mode: {
          type: "string",
          enum: ["call", "detach"],
          description: "'call' waits for result, 'detach' returns a handle",
        },
      },
      required: ["agentId", "prompt"],
    },
    executor: { kind: "orchestrator", target: "orchestrator" },
    capabilities: ["delegate_agent"],
  },
  {
    name: "join_subtask",
    description: "Join a previously detached subagent task and get its result.",
    inputSchema: {
      type: "object",
      properties: {
        taskId: {
          type: "string",
          description: "Task ID returned by delegate_to_agent in detach mode",
        },
      },
      required: ["taskId"],
    },
    executor: { kind: "orchestrator", target: "orchestrator" },
    capabilities: ["delegate_agent"],
  },
  {
    name: "get_orchestrator_state",
    description: "Read the current orchestrator state snapshot.",
    inputSchema: {
      type: "object",
      properties: {
        format: {
          type: "string",
          enum: ["snapshot", "graph"],
          description: "Output format",
        },
      },
    },
    executor: { kind: "orchestrator", target: "orchestrator" },
    capabilities: ["read_workspace"],
  },
  {
    name: "update_plan",
    description: "Update the current agent task plan.",
    inputSchema: {
      type: "object",
      properties: {
        plan: {
          type: "array",
          items: { type: "object" },
          description: "Task plan steps",
        },
      },
      required: ["plan"],
    },
    executor: { kind: "orchestrator", target: "orchestrator" },
    capabilities: ["update_plan"],
  },
];

// ---- Provider implementation ----

export class OrchToolProvider implements ToolProvider {
  id = "orch";
  source = "orch" as const;

  private orchestrator: Orchestrator;

  constructor(orchestrator: Orchestrator) {
    this.orchestrator = orchestrator;
  }

  async discover(_context: ToolDiscoveryContext): Promise<ToolDef[]> {
    return ORCH_TOOLS;
  }

  async execute(call: ToolCall, context: ToolExecutionContext): Promise<ToolExecResult> {
    switch (call.name) {
      case "delegate_to_agent":
        return this.handleDelegate(call, context);
      case "join_subtask":
        return this.handleJoin(call);
      case "get_orchestrator_state":
        return this.handleGetState(call);
      case "update_plan":
        return this.handleUpdatePlan(call, context);
      default:
        return {
          ok: false,
          error: {
            code: "unknown_tool",
            message: `Unknown orchestrator tool: ${call.name}`,
          },
        };
    }
  }

  private async handleDelegate(
    call: ToolCall,
    context: ToolExecutionContext,
  ): Promise<ToolExecResult> {
    const agentId = typeof call.arguments.agentId === "string" ? call.arguments.agentId : undefined;
    const prompt = typeof call.arguments.prompt === "string" ? call.arguments.prompt : undefined;
    const mode = (typeof call.arguments.mode === "string" ? call.arguments.mode : "call") as
      | "call"
      | "detach";

    if (!agentId || !prompt) {
      return {
        ok: false,
        error: {
          code: "invalid_args",
          message: "delegate_to_agent requires agentId and prompt",
        },
      };
    }

    const task = {
      targetAgentId: agentId,
      prompt,
      source: {
        type: "agent" as const,
        agentId: context.agentId,
        taskId: context.taskId,
      },
      parentTaskId: context.taskId,
    };

    if (mode === "detach") {
      try {
        const taskId = await this.orchestrator.dispatchDetached(task);
        return {
          ok: true,
          value: {
            delegated: true,
            taskId,
            targetAgentId: agentId,
            mode: "detach",
          },
        };
      } catch (err) {
        return {
          ok: false,
          error: {
            code: "delegation_failed",
            message: err instanceof Error ? err.message : "Delegation failed",
          },
        };
      }
    }

    // "call" mode: detach then immediately join
    try {
      const taskId = await this.orchestrator.dispatchDetached(task);
      const result = await this.orchestrator.joinTask(taskId);
      return {
        ok: true,
        value: {
          delegated: true,
          taskId,
          targetAgentId: agentId,
          mode: "call",
          result,
        },
      };
    } catch (err) {
      return {
        ok: false,
        error: {
          code: "delegation_failed",
          message: err instanceof Error ? err.message : "Delegation failed",
        },
      };
    }
  }

  private async handleJoin(call: ToolCall): Promise<ToolExecResult> {
    const taskId = typeof call.arguments.taskId === "string" ? call.arguments.taskId : undefined;
    if (!taskId) {
      return {
        ok: false,
        error: {
          code: "invalid_args",
          message: "join_subtask requires taskId",
        },
      };
    }

    try {
      const result = await this.orchestrator.joinTask(taskId);
      return {
        ok: true,
        value: { joined: true, taskId, result },
      };
    } catch (err) {
      return {
        ok: false,
        error: {
          code: "join_failed",
          message: err instanceof Error ? err.message : "Join failed",
        },
      };
    }
  }

  private async handleGetState(call: ToolCall): Promise<ToolExecResult> {
    try {
      const format = typeof call.arguments.format === "string" ? call.arguments.format : "snapshot";

      if (format === "graph") {
        // Graph rendering not implemented on facade yet; fall back to snapshot
        const snapshot = this.orchestrator.snapshot();
        return { ok: true, value: { graph: snapshot } };
      }

      const snapshot = this.orchestrator.snapshot();
      return { ok: true, value: { snapshot } };
    } catch (err) {
      return {
        ok: false,
        error: {
          code: "state_read_failed",
          message: err instanceof Error ? err.message : "Failed to read orchestrator state",
        },
      };
    }
  }

  private async handleUpdatePlan(
    call: ToolCall,
    context: ToolExecutionContext,
  ): Promise<ToolExecResult> {
    const plan = Array.isArray(call.arguments.plan) ? call.arguments.plan : [];

    try {
      this.orchestrator.updatePlan(context.agentId, context.taskId, plan);
    } catch {
      // Non-fatal: plan update is best effort
    }

    return { ok: true, value: { updated: true, plan } };
  }
}
