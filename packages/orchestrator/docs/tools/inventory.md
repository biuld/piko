# Tool Inventory

Inventory of piko preset tools, grouped by provider. Detailed provider,
ToolSet, routing, and execution semantics live in the other tools docs.

## Columns

| Column | Meaning |
| --- | --- |
| Tool | Provider-visible tool name |
| Default ToolSet | Preset ToolSet membership |
| Sensitivity | Safe, sensitive, or dangerous |
| Approval | Default approval policy |
| Notes | Short behavior summary |

## Orchestrator Provider

Actor-control tools implemented by `OrchestratorToolProvider`.

`update_plan` is listed here because plans are agent task state owned by
Orchestrator, not Host/TUI global UI state.

| Tool | Default ToolSet | Sensitivity | Approval | Notes |
| --- | --- | --- | --- | --- |
| `delegate_to_agent` | coordinator, implementer optional | sensitive | on_sensitive | Start subagent in `call` or `detach` mode |
| `join_subtask` | coordinator, implementer optional | safe | never | Join a detached subagent handle |
| `get_orchestrator_state` | coordinator, diagnostics | safe | never | Read snapshot or graph projection |
| `update_plan` | coordinator, implementer | safe | never | Update the current agent task plan |

## Engine Provider

Low-level workspace/system tools implemented by `EngineToolProvider`.

| Tool | Default ToolSet | Sensitivity | Approval | Notes |
| --- | --- | --- | --- | --- |
| `shell` | implementer, diagnostics | dynamic | on_sensitive | Run workspace shell command |
| `apply_patch` | implementer | sensitive | on_sensitive | Apply patch to workspace files |
| `grep` | read-only, implementer, diagnostics | safe | never | Search file contents |
| `find` | read-only, implementer, diagnostics | safe | never | Find files by path/name |
| `ls` | read-only, implementer, diagnostics | safe | never | List directory contents |
| `read` | read-only, implementer, diagnostics | safe | never | Read file contents |
| `view_image` | implementer, diagnostics | safe | never | Inspect local image file |

## Host Provider

Host/TUI bridge tools implemented by `HostToolProvider`.

| Tool | Default ToolSet | Sensitivity | Approval | Notes |
| --- | --- | --- | --- | --- |
| `ask_user` | coordination | safe | never | Ask the user a direct question through Host/TUI |
| `request_approval` | coordination | safe | never | Ask Host/user for approval not tied to a ToolActor policy |
| `request_user_input` | coordination | safe | never | Ask Host/TUI for user input |
| `open_external` | explicit host tools | sensitive | always | Ask Host to open URL/file/app |

## Future Providers

| Provider | Notes |
| --- | --- |
| MCP | Dynamic external tools exposed through MCP servers |
| Plugin | Plugin-contributed tools and ToolSets |
