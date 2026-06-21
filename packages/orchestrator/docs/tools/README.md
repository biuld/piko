# Tools

Tools are a first-class orchestration surface. They are discovered through `ToolProvider`s, constrained by `ToolSet`s, executed by `ToolRegistryImpl.executeTool()`, and exposed to the model as model-visible tool definitions.

There are two broad categories:

1. capability tools, such as shell, patch, image view, MCP, or host-provided tools
2. actor-control tools, such as delegating to subagents, joining subtasks, updating plans, or reading Orchestrator state

These categories share the same discovery and execution path:

```text
ToolProvider -> ToolRegistry discovery -> ToolSet filtering/policy -> model-visible tools
Model tool call -> AgentActor -> ToolRegistryImpl.executeTool() -> selected ToolProvider
```

## Topics

- [Providers](providers.md) - `ToolProvider` API and tool source boundaries.
- [ToolSets](toolsets.md) - agent capability boundaries and policy.
- [Tool Inventory](inventory.md) - current piko preset tools and planned ownership, including Orchestrator actor-control tools.

## Ownership Summary

| Concern | Owner |
| --- | --- |
| Provider discovery/execution adapter | `ToolProvider` |
| Agent capability boundary | `ToolSet` |
| Tool catalog computation | `ToolRegistry` (synchronous lookup and filtering) |
| Approval and tool lifecycle coordination | `ToolRegistryImpl.executeTool()` (stateless method) |
| Actor-control tool implementation, including per-task plan updates | `OrchToolProvider` (in `orchestrator`, auto-registered) |
| Host/TUI bridge tools | `HostToolProvider` (in `host-runtime`) |
| Low-level shell/file/search tools | `WorkspaceToolProvider` (in `host-runtime`, ID `"workspace"`) |
