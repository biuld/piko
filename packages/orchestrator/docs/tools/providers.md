# Tool Providers

Tools are registered through `ToolProvider`s. A provider is the discovery and
execution adapter for one source of tools.

```ts
export interface ToolProvider {
  id: string;
  source: "orch" | "host" | "workspace" | "mcp" | "plugin";

  discover(context: ToolDiscoveryContext): Promise<ToolDefinition[]>;
  execute(call: ToolCall, context: ToolExecutionContext): Promise<ToolResult>;
}
```

Discovery returns tool definitions and policy metadata. Execution performs the
provider-specific action. `ToolRegistryImpl.executeTool()` owns coordination
around the provider: approval, lifecycle events, timeout, cancellation, and
structured results.

## Sources

| Source | Provider (id) | Lives in | Owns |
| --- | --- | --- | --- |
| Orchestrator | `OrchToolProvider` (`"orch"`) | `orchestrator/src/tools/orch-provider.ts` (auto-registered by `Orchestrator` constructor) | actor-control tools: delegation, join, plan updates, state read |
| Host | `HostToolProvider` (`"host"`) | `host-runtime` | model-visible UI/session bridge tools: user questions, explicit approval requests |
| Workspace | `WorkspaceToolProvider` (`"workspace"`) | `host-runtime` (or future `engine-rs`) | low-level workspace/system tools: shell, grep, ls, file read, patch |
| MCP | `McpToolProvider` (`"mcp:<serverName>"`) | `host-runtime` | external dynamic capabilities via Model Context Protocol (MCP) servers |
| Plugin | future providers | TBD | plugin-contributed external capabilities |

The model should not talk to Host/TUI directly. If a model-visible tool needs
Host or TUI behavior, Host should expose it through `HostToolProvider`.
Orchestrator then sees it as a normal provider-backed tool and can still apply
eventing, approval policy, and cancellation.

Tool approval is not routed through `HostToolProvider`; it calls
the Host-provided `ApprovalGateway` directly.

Workspace-owned low-level tools are intentionally behind `WorkspaceToolProvider`
(or a future `engine-rs` provider). Moving shell/file execution into a stronger
system sandbox won't change AgentActor or ToolRegistryImpl execution semantics.
