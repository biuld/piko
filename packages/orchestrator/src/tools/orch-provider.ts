// ---- OrchToolProvider — orchestrator control tools ----
// Built-in provider for actor-control tools: delegation, join, state read, plan update.
//
// Registered automatically by the Orchestrator facade with id "orch".
// ToolRegistryImpl discovers orchestrator_control refs by looking up the "orch" provider.

import type {
  ToolCall,
  ToolDef,
  ToolDiscoveryContext,
  ToolExecResult,
  ToolExecutionContext,
  ToolProvider,
} from "piko-orchestrator-protocol";
import type { Orchestrator } from "../orchestrator/index.js";

// ---- Tool definitions ----

const ORCH_TOOLS: ToolDef[] = [
  {
    name: "delegate_to_agent",
    description:
      "Delegate a task to a subagent in 'call' (blocking) or 'detach' (fire-and-forget with later join) mode.",
    inputSchema: {
      type: "object",
      properties: {
        agentId: { type: "string", description: "Target agent ID" },
        prompt: { type: "string", description: "Task description for the subagent" },
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
        format: { type: "string", enum: ["snapshot", "graph"], description: "Output format" },
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
        plan: { type: "array", items: { type: "object" }, description: "Task plan steps" },
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
          error: { code: "unknown_tool", message: `Unknown orchestrator tool: ${call.name}` },
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
        error: { code: "invalid_args", message: "delegate_to_agent requires agentId and prompt" },
      };
    }

    const snapshot = this.orchestrator.snapshot();
    if (!snapshot.agents[agentId]) {
      return {
        ok: false,
        error: {
          code: "not_found",
          message: `Agent "${agentId}" is not registered. Available agents: ${Object.keys(snapshot.agents).join(", ") || "(none)"}`,
        },
      };
    }

    if (agentId === context.agentId) {
      return {
        ok: false,
        error: {
          code: "invalid_args",
          message: `Cannot delegate to yourself (${agentId}). Delegate to a different agent.`,
        },
      };
    }

    const targetAgent = snapshot.agents[agentId];
    const activeTasks = Object.values(snapshot.tasks).filter(
      (t) => t.targetAgentId === agentId && (t.status === "running" || t.status === "queued"),
    );
    const limit = targetAgent.spec?.concurrency?.maxConcurrentTasks;
    const isBusy =
      limit !== undefined ? activeTasks.length >= limit : targetAgent.status === "running";

    if (isBusy) {
      return {
        ok: false,
        error: {
          code: "agent_busy",
          message: `Agent "${agentId}" is currently running a task.`,
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
        const taskId = await this.orchestrator.delegateDetached(task);
        return {
          ok: true,
          value: { delegated: true, taskId, targetAgentId: agentId, mode: "detach" },
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

    // "call" mode waits directly on the task-scoped AgentActor result.
    try {
      const { taskId, result } = await this.orchestrator.delegateToAgent(task);
      return {
        ok: true,
        value: { delegated: true, taskId, targetAgentId: agentId, mode: "call", result },
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
        error: { code: "invalid_args", message: "join_subtask requires taskId" },
      };
    }

    try {
      const result = await this.orchestrator.joinTask(taskId);
      return { ok: true, value: { joined: true, taskId, result } };
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
        const graph = await this.orchestrator.getGraph();
        return { ok: true, value: { graph } };
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
