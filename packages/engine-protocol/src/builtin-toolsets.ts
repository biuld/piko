import type { EngineToolSet } from "./tools.js";

// ---------------------------------------------------------------------------
// Built-in ToolSets that are Host- or Orchestrator-mediated.
//
// These define the protocol-level tool surface. The actual executors are
// resolved at runtime by the Host (for host-mediated tools) or the
// Orchestrator (for orchestrator tools).
// ---------------------------------------------------------------------------

/**
 * Planning ToolSet — lets the agent update a structured task plan.
 * Executor kind: "host" — the Host renders the plan in the TUI.
 */
export const planningToolSet: EngineToolSet = {
  id: "builtin:planning",
  name: "Planning",
  description: "Update the visible task plan.",
  tools: [
    {
      name: "update_plan",
      description: "Update the visible task plan with step statuses.",
      inputSchema: {
        type: "object",
        properties: {
          explanation: {
            type: "string",
            description: "Optional explanation of the plan update.",
          },
          plan: {
            type: "array",
            items: {
              type: "object",
              properties: {
                step: { type: "string", description: "Step description" },
                status: {
                  type: "string",
                  enum: ["pending", "in_progress", "completed"],
                  description: "Step status",
                },
              },
              required: ["step", "status"],
            },
            description: "The updated plan steps.",
          },
        },
        required: ["plan"],
      },
      executor: { kind: "host", target: "update_plan" },
      exposure: "direct",
      capabilities: ["update_plan"],
      approval: "never",
    },
  ],
};

/**
 * Discovery ToolSet — lets the agent search for deferred tools.
 * Executor kind: "orchestrator" — the orchestrator searches registered ToolSets.
 */
export const discoveryToolSet: EngineToolSet = {
  id: "builtin:discovery",
  name: "Tool Discovery",
  description: "Search for tools available in this session.",
  tools: [
    {
      name: "tool_search",
      description: "Search deferred tools available in this session by name or description.",
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
    },
  ],
};

/**
 * Delegation ToolSet — lets a coordinator agent delegate to another agent.
 * Executor kind: "orchestrator" — the orchestrator enqueues the task.
 */
export const delegationToolSet: EngineToolSet = {
  id: "builtin:delegation",
  name: "Agent Delegation",
  description: "Delegate a task to another registered agent.",
  tools: [
    {
      name: "delegate_to_agent",
      description: "Delegate a task to another registered agent. The task will be enqueued.",
      inputSchema: {
        type: "object",
        properties: {
          agentId: { type: "string", description: "Target agent ID to delegate to" },
          prompt: { type: "string", description: "Task description for the agent" },
          priority: {
            type: "number",
            description: "Task priority (higher = more important, default 0)",
          },
        },
        required: ["agentId", "prompt"],
      },
      executor: { kind: "orchestrator", target: "delegate_to_agent" },
      exposure: "direct",
      capabilities: ["delegate_agent"],
      approval: "never",
    },
  ],
};

/**
 * View Image ToolSet — lets the agent view image files.
 * Executor kind: "host" — the Host renders the image in the TUI.
 */
export const viewImageToolSet: EngineToolSet = {
  id: "builtin:view-image",
  name: "View Image",
  description: "View image files in the workspace.",
  tools: [
    {
      name: "view_image",
      description: "View an image file. Supported formats: jpg, png, gif, webp.",
      inputSchema: {
        type: "object",
        properties: {
          path: { type: "string", description: "Path to the image file" },
          detail: {
            type: "string",
            enum: ["high", "original"],
            description: "Image detail level (default: high)",
          },
        },
        required: ["path"],
      },
      executor: { kind: "host", target: "view_image" },
      exposure: "direct",
      capabilities: ["view_image"],
      approval: "never",
    },
  ],
};

// ---- Default team ToolSet presets ----

/**
 * Coordinator agent toolset: planning + discovery + delegation.
 */
export const coordinatorToolSetIds = [
  planningToolSet.id,
  discoveryToolSet.id,
  delegationToolSet.id,
];

/**
 * Implementer agent toolset: core-coding + planning.
 */
export const implementerToolSetIds: string[] = ["builtin:core-coding", planningToolSet.id];

/**
 * Reviewer agent toolset: read-only shell only.
 */
export const reviewerToolSetIds: string[] = ["builtin:read-only-shell"];
