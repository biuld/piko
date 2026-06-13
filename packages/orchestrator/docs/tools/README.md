# Tools

Tools are a first-class orchestration surface. They are discovered through
`ToolProvider`s, constrained by `ToolSet`s, coordinated by `ToolActor`, and
exposed to Engine as provider-visible tool definitions.

There are two broad categories:

1. capability tools, such as shell, patch, image view, MCP, or host-provided
   tools
2. actor-control tools, such as delegating to subagents, joining subtasks,
   updating plans, or reading Orchestrator state

These categories share the same discovery and execution path:

```text
ToolProvider -> ToolActor discovery -> ToolSet filtering/policy -> Engine tools
Engine tool call -> AgentActor -> ToolActor -> selected ToolProvider
```

## Topics

- [Providers](providers.md) - `ToolProvider` API and tool source boundaries.
- [ToolSets](toolsets.md) - agent capability boundaries and policy.
- [ToolActor](../actors/tool-actor.md) - discovery, routing, execution
  coordination.
- [Tool Inventory](inventory.md) - current piko preset tools and planned
  ownership, including Orchestrator actor-control tools.

## Ownership Summary

| Concern | Owner |
| --- | --- |
| Provider discovery/execution adapter | `ToolProvider` |
| Agent capability boundary | `ToolSet` |
| Tool catalog computation | `ToolActor` |
| Approval and tool lifecycle coordination | `ToolActor` |
| Actor-control tool implementation, including per-task plan updates | `OrchToolProvider` (in `orchestrator`, auto-registered) |
| Host/TUI bridge tools | `HostToolProvider` |
| Low-level shell/file/search tools | `WorkspaceToolProvider` (in `host-runtime`) |
