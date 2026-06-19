# Tool Inventory

Inventory of piko preset tools, grouped by provider. Detailed provider, ToolSet, routing, and execution semantics live in the other tools docs.

## Columns

| Column | Meaning |
| --- | --- |
| Tool | Provider-visible tool name |
| Default ToolSet | ToolSet membership |
| Sensitivity | Safe, sensitive, or dangerous |
| Approval | Default approval policy |
| Notes | Short behavior summary |

## Orchestrator Provider

Actor-control tools implemented by `OrchToolProvider` (lives in `orchestrator/src/tools/orch-provider.ts`, registered with `id: "orch"`, `source: "orch"`).

`update_plan` is listed here because plans are agent task state owned by Orchestrator, not Host/TUI global UI state.

| Tool | Default ToolSet | Sensitivity | Approval | Notes |
| --- | --- | --- | --- | --- |
| `delegate_to_agent` | `builtin` | sensitive | on_sensitive | Start subagent in `call` or `detach` mode |
| `join_subtask` | `builtin` | safe | never | Join a detached subagent handle |
| `get_orchestrator_state` | `builtin` | safe | never | Read snapshot or graph projection |
| `update_plan` | `builtin` | safe | never | Update the current agent task plan |

## Workspace Provider

Low-level workspace/system tools implemented by `WorkspaceToolProvider` (in `host-runtime`), registered with `id: "workspace"`, `source: "workspace"`.

| Tool | Default ToolSet | Sensitivity | Approval | Notes |
| --- | --- | --- | --- | --- |
| `read` | `builtin` | safe | never | Read file contents (text or image) |
| `bash` | `builtin` | dangerous | always | Run workspace shell command |
| `edit` | `builtin` | dangerous | always | Apply replacement edit to workspace files |
| `write` | `builtin` | dangerous | always | Write whole content to a workspace file |
| `grep` | `builtin` | safe | never | Search file contents |
| `find` | `builtin` | safe | never | Find files by path/name pattern |
| `ls` | `builtin` | safe | never | List directory contents |
| `view_image` | `builtin` | safe | never | Inspect local image file |

## Host Provider

Host/TUI bridge tools implemented by `HostToolProvider` (in `host-runtime`), registered with `id: "host"`, `source: "host"`. These are available for registration in custom/dynamic toolsets.

| Tool | Default ToolSet | Sensitivity | Approval | Notes |
| --- | --- | --- | --- | --- |
| `ask_user` | optional | safe | never | Ask the user a direct question through Host/TUI |
| `request_approval` | optional | safe | never | Ask Host/user for approval not tied to a tool approval policy |
| `request_user_input` | optional | safe | never | Ask Host/TUI for arbitrary user input |
| `open_external` | optional | sensitive | always | Ask Host to open URL/file/app |

## MCP Provider

Dynamic external capabilities exposed via Model Context Protocol (MCP) servers, implemented by `McpToolProvider` (in `host-runtime`), registered with `id: "mcp:<serverName>"`, `source: "mcp"`. Available tools are discovered dynamically from each connected MCP server.

## Future Providers

| Provider | Notes |
| --- | --- |
| Plugin | Plugin-contributed tools and ToolSets |
