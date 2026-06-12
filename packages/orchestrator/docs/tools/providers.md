# Tool Providers

Tools are registered through `ToolProvider`s. A provider is the discovery and
execution adapter for one source of tools.

```ts
export interface ToolProvider {
  id: string;
  source: "orchestrator" | "host" | "engine" | "mcp" | "plugin";

  discover(context: ToolDiscoveryContext): Promise<ToolDefinition[]>;
  execute(call: ToolCall, context: ToolExecutionContext): Promise<ToolResult>;
}
```

Discovery returns tool definitions and policy metadata. Execution performs the
provider-specific action. `ToolActor` owns coordination around the provider:
approval, lifecycle events, timeout, cancellation, and structured results.

## Sources

| Source | Provider | Owns |
| --- | --- | --- |
| Orchestrator | `OrchestratorToolProvider` | actor-control tools such as delegation, join, plan updates, state |
| Host | `HostToolProvider` | UI/session/user-facing bridge tools, user questions, approvals |
| Engine | `EngineToolProvider` | low-level workspace/system tools such as shell, grep, ls, file read, patch |
| MCP/plugin | future providers | external dynamic capabilities |

The model should not talk to Host/TUI directly. If a model-visible tool needs
Host or TUI behavior, Host should expose it through `HostToolProvider`.
Orchestrator then sees it as a normal provider-backed tool and can still apply
eventing, approval policy, and cancellation.

Engine-owned low-level tools are intentionally behind `EngineToolProvider`.
Later, an `engine-rs` provider can move shell/file execution into a stronger
system sandbox without changing AgentActor or ToolActor semantics.
