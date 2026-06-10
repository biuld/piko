import type { EngineTool, EngineToolSet } from "./tools.js";

// ============================================================================
// Standard tool definitions — defined ONCE.
//
// These are the built-in tools available to the harness agent.
// ToolSet is an extension point: users can define narrower subsets
// for specialized sub-agents (coordinator, reviewer, tester, etc.).
// ============================================================================

const shellTool: EngineTool = {
  name: "shell",
  description:
    "Execute a shell command in the workspace. Use cat, rg, fd, ls, find for reading; use git, npm, etc. for operations.",
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
};

const applyPatchTool: EngineTool = {
  name: "apply_patch",
  description:
    "Apply a structured patch to files in the workspace. Use *** Begin Patch / *** End Patch grammar.",
  inputSchema: {
    type: "object",
    properties: { patch: { type: "string", description: "Patch content" } },
    required: ["patch"],
  },
  executor: { kind: "native", target: "apply_patch" },
  executionMode: "sequential",
  exposure: "direct",
  capabilities: ["write_workspace"],
  approval: "always",
};

const updatePlanTool: EngineTool = {
  name: "update_plan",
  description: "Update the visible task plan with step statuses.",
  inputSchema: {
    type: "object",
    properties: {
      explanation: { type: "string", description: "Optional explanation." },
      plan: {
        type: "array",
        items: {
          type: "object",
          properties: {
            step: { type: "string" },
            status: { type: "string", enum: ["pending", "in_progress", "completed"] },
          },
          required: ["step", "status"],
        },
      },
    },
    required: ["plan"],
  },
  executor: { kind: "host", target: "update_plan" },
  exposure: "direct",
  capabilities: ["update_plan"],
  approval: "never",
};

const viewImageTool: EngineTool = {
  name: "view_image",
  description: "View an image file. Supported formats: jpg, png, gif, webp.",
  inputSchema: {
    type: "object",
    properties: {
      path: { type: "string", description: "Path to the image file" },
      detail: { type: "string", enum: ["high", "original"], description: "Image detail level" },
    },
    required: ["path"],
  },
  executor: { kind: "host", target: "view_image" },
  exposure: "direct",
  capabilities: ["view_image"],
  approval: "never",
};

const toolSearchTool: EngineTool = {
  name: "tool_search",
  description: "Search for available tools by name or description.",
  inputSchema: {
    type: "object",
    properties: {
      query: { type: "string", description: "Search query" },
      limit: { type: "number", description: "Maximum results (default 10)" },
    },
    required: ["query"],
  },
  executor: { kind: "orchestrator", target: "tool_search" },
  exposure: "direct",
  capabilities: ["discover_tools"],
  approval: "never",
};

const delegateToAgentTool: EngineTool = {
  name: "delegate_to_agent",
  description: "Delegate a task to another registered agent.",
  inputSchema: {
    type: "object",
    properties: {
      agentId: { type: "string", description: "Target agent ID" },
      prompt: { type: "string", description: "Task description" },
      priority: { type: "number", description: "Priority (default 0)" },
    },
    required: ["agentId", "prompt"],
  },
  executor: { kind: "orchestrator", target: "delegate_to_agent" },
  exposure: "direct",
  capabilities: ["delegate_agent"],
  approval: "never",
};

// ============================================================================
// Built-in ToolSet — all tools available to the default harness agent.
// ============================================================================

export const builtinToolSet: EngineToolSet = {
  id: "builtin",
  name: "Built-in",
  tools: [
    shellTool,
    applyPatchTool,
    updatePlanTool,
    viewImageTool,
    toolSearchTool,
    delegateToAgentTool,
  ],
};
