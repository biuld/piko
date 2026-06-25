import type { ToolSet } from "piko-orch-protocol";

export const builtinToolSet: ToolSet = {
  id: "builtin",
  name: "Built-in",
  tools: [
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "read",
      policy: { sensitivity: "safe", approval: "never" },
    },
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "bash",
      policy: { sensitivity: "dangerous", approval: "always" },
    },
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "edit",
      policy: { sensitivity: "dangerous", approval: "always" },
    },
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "write",
      policy: { sensitivity: "dangerous", approval: "always" },
    },
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "grep",
      policy: { sensitivity: "safe", approval: "never" },
    },
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "find",
      policy: { sensitivity: "safe", approval: "never" },
    },
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "ls",
      policy: { sensitivity: "safe", approval: "never" },
    },
    {
      kind: "provider_tool",
      providerId: "workspace",
      toolName: "view_image",
      policy: { sensitivity: "safe", approval: "never" },
    },
    {
      kind: "provider_namespace",
      providerId: "host",
      namespace: "",
      policy: { sensitivity: "safe", approval: "never" },
    },
    {
      kind: "orchestrator_control",
      action: "get_orchestrator_state",
      policy: { sensitivity: "safe", approval: "never" },
    },
    {
      kind: "orchestrator_control",
      action: "update_plan",
      policy: { sensitivity: "safe", approval: "never" },
    },
    {
      kind: "orchestrator_control",
      action: "delegate_to_agent",
      policy: { sensitivity: "sensitive", approval: "on_sensitive" },
    },
    {
      kind: "orchestrator_control",
      action: "join_subtask",
      policy: { sensitivity: "safe", approval: "never" },
    },
  ],
};

export const builtinToolNames = new Set(
  builtinToolSet.tools
    .filter(
      (tool): tool is Extract<(typeof builtinToolSet.tools)[number], { kind: "provider_tool" }> =>
        tool.kind === "provider_tool",
    )
    .map((tool) => tool.toolName),
);
