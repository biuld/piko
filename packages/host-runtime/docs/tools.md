# Tool Providers in Host Runtime

The `host-runtime` package implements the core capabilities that are exposed to agents as tool definitions. It provides three primary `ToolProvider` implementations that integrate with the Orchestrator.

## 1. Workspace Tool Provider (`WorkspaceToolProvider`)

- **ID**: `workspace`
- **Source**: `workspace`
- **Implementation**: [workspace-provider.ts](file:///Users/biu/Projects/piko/packages/host-runtime/src/tools/workspace-provider.ts)
- **Description**: Exposes low-level filesystem and shell execution capabilities within the current session's execution environment (`ExecutionEnv`).

### Provided Tools

| Tool Name | Sensitivity | Default Approval | Purpose / Behavior |
| --- | --- | --- | --- |
| `read` | `safe` | `never` | Reads file content. Supports both text and images (jpg, png, gif, webp). |
| `bash` | `dangerous` | `always` | Executes a shell command in the session's workspace directory. |
| `edit` | `dangerous` | `always` | Applies non-overlapping string replacement edits to a file. |
| `write` | `dangerous` | `always` | Writes whole content to a workspace file. Automatically creates directories. |
| `grep` | `safe` | `never` | Searches file contents using ripgrep. |
| `find` | `safe` | `never` | Finds files matching a path/name pattern. |
| `ls` | `safe` | `never` | Lists directory contents. |
| `view_image` | `safe` | `never` | Inspects a local image file. |

---

## 2. Host Tool Provider (`HostToolProvider`)

- **ID**: `host`
- **Source**: `host`
- **Implementation**: [host-provider.ts](file:///Users/biu/Projects/piko/packages/host-runtime/src/tools/host-provider.ts)
- **Description**: Acts as a bridge between the agent runtime (Orchestrator) and the UI/TUI layer (Host). It converts model-visible tool calls into Host callbacks.

### Provided Tools

| Tool Name | Sensitivity | Default Approval | Purpose / Behavior |
| --- | --- | --- | --- |
| `ask_user` | `safe` | `never` | Sends a prompt to the TUI to ask the user a direct question. |
| `request_approval` | `safe` | `never` | Requests generic user confirmation/approval for non-policy actions. |
| `request_user_input` | `safe` | `never` | Prompts the user for arbitrary input (text, confirm, or choice). |
| `open_external` | `sensitive` | `always` | Opens a URL, file path, or external application. |

---

## 3. MCP Tool Provider (`McpToolProvider`)

- **ID**: `mcp:<serverName>`
- **Source**: `mcp`
- **Implementation**: [mcp-provider.ts](file:///Users/biu/Projects/piko/packages/host-runtime/src/tools/mcp-provider.ts)
- **Description**: Dynamically connects to external Model Context Protocol (MCP) servers.

### Integration Details
- Managed by `McpServerManager` inside the Host runtime during initialization.
- Subscribes to the official MCP SDK client.
- Discovers schema definitions from each server and exposes them as model-visible tool definitions.
- Automatically handles routing and execution of MCP tool calls.
- Tool sensitivity is configured as `sensitive` with `on_sensitive` (on request) approval policy by default.
