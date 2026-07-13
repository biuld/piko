# Tool Sets Design

> Status: implemented
> Related: [Multi-Agent Runtime Model](multi-agent-execution-model.md) §7 LLM Tool Boundary
> Protocol types: `piko_protocol::tools::{ToolDef, ToolSet, ToolSetToolRef}`

## 1. Purpose

Define how tools are **grouped**, **declared on agents**, and **registered at
runtime**, so AgentSpec TOML, orchd discovery, and what the model sees stay
aligned.

This doc does not invent a new registry abstraction. `ToolProvider` remains the
implementation unit; `ToolSet` remains the agent-facing pack id.

## 2. Layers

```text
ToolProvider          discovers/executes concrete tools (todo, workspace, …)
    ↓ referenced by
ToolSet               named pack exposed to AgentSpec (`todo`, `workspace`, …)
    ↓ selected by
AgentSpec.tool_set_ids
    ↓ optionally narrowed by
AgentSpec.active_tool_names / settings.active_tool_names
    ↓
GatewayRequest.tools  (what the LLM receives)
```

| Layer | Answers |
|---|---|
| Provider | How is a tool implemented? |
| ToolSet | Which capability pack does an agent opt into? |
| `tool_set_ids` | Which packs does this AgentSpec include? |
| `active_tool_names` | Optional allow-list over the discovered catalog |

Rules:

1. Agents declare **tool sets**, not individual provider ids.
2. A ToolSet id is stable product vocabulary; it should match the capability name
   (`todo`, `workspace`, …).
3. Missing ToolSet id ⇒ those tools are absent from the model request.
4. `active_tool_names = Some([...])` further filters; `None` means all tools
   from selected sets.

## 3. Canonical Tool Sets

Capability-oriented packs. One concern per set.

### 3.1 Inventory

| ToolSet id | Provider id | Owner | Tools | Capability |
|---|---|---|---|---|
| `todo` | `todo` | orchd (Execution bootstrap) | `todo_write`, `todo_read` | plan / task list |
| `workspace` | `workspace` | orchd (Execution bootstrap) | `read`, `bash`, `edit`, `write` | local FS + shell |
| `user_interaction` | `user_interaction` | hostd (per Turn wiring) | `ask_user`, `request_user_input` | human-in-the-loop |
| `multi_agent` | `multi_agent` | orchd (`AgentRuntime::bootstrap`) | `spawn_agent`, `spawn_agent_detached`, `send_agent_message`, `get_agent_status`, `collect_agent_reports`, `close_agent`, `reopen_agent` | AgentInstance delegation |
| `mcp_<server>` | MCP server name | hostd (MCP init) | server-defined | external integrations |

### 3.2 Grouping rationale

```text
                    ┌─────────────┐
                    │  AgentSpec  │
                    └──────┬──────┘
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
      ┌────────┐     ┌──────────┐    ┌────────────┐
      │  todo  │     │workspace │    │multi_agent │
      │ plan   │     │ local IO │    │ delegation │
      └────────┘     └──────────┘    └────────────┘
           │               │               │
           └───────┬───────┴───────┬───────┘
                   ▼               ▼
            ┌────────────┐  ┌──────────────────┐
            │user_inter… │  │ mcp_<server>     │
            │ ask human  │  │ external (opt-in)│
            └────────────┘  └──────────────────┘
```

- **`todo`**: ephemeral planning state; no workspace side effects.
- **`workspace`**: sandboxed local mutation / observation.
- **`user_interaction`**: blocks on host/TUI; only meaningful when host
  callbacks are wired.
- **`multi_agent`**: AgentRuntime control surface; never mixed into workspace.
- **`mcp_*`**: one ToolSet per configured MCP server; opt-in via agent or
  session config, not silent global attach.

## 4. Default Agent Matrix

Built-in AgentSpecs under `packages/hostd/resources/agents/`:

| Agent | Role | Default `tool_set_ids` |
|---|---|---|
| `main` | root | `todo`, `workspace`, `user_interaction`, `multi_agent` |
| `general` | worker | `todo`, `workspace`, `multi_agent` |
| `coder` | worker | `todo`, `workspace`, `multi_agent` |
| `scout` | worker | `todo`, `workspace`, `multi_agent` |

Notes:

- Root needs `user_interaction`; child workers normally do not.
- Workers keep `multi_agent` so they can spawn further children under policy
  limits (depth / count enforced by AgentRuntime).
- MCP sets are never in the built-in defaults; host attaches them when
  configured.

## 5. Ownership and Registration

| ToolSet | When registered | Where |
|---|---|---|
| `todo` | Execution runtime bootstrap | `AgentExecutionRuntime::register_single_agent_tools` |
| `workspace` | same | same |
| `multi_agent` | AgentRuntime bootstrap | `AgentRuntime::bootstrap` |
| `user_interaction` | each Turn, after host callbacks are set | `OrchTurnRunner` |
| `mcp_*` | orch runner construction | `initialize_mcp_tools` |

Invariant: **declaring a ToolSet id on an AgentSpec without registering that
ToolSet yields no tools** (silent empty contribution). Registration must happen
before the first model step that needs them.

## 6. Source of Truth and Injection

### 6.1 Source of truth

`AgentSpec.tool_set_ids` in agent TOML (or an equivalent registered AgentSpec)
is the product declaration. Operators and authors should read the TOML to know
what the model can call.

### 6.2 Limited root ensure

hostd may **ensure** root mandatory packs when assembling a live root turn
spec (`user_interaction`, `multi_agent`) if a custom project agent omits them.
That is a safety net, not the primary declaration.

Policy:

1. Prefer fixing TOML / registered specs over silent injection.
2. Ensure only runs for the root turn agent; it only **adds** missing set ids.
3. Attach / recovery prefer the **registered** AgentSpec (turn-prepared root,
   registry snapshot) over a stale recovery copy rebuilt from raw TOML.
4. orchd does **not** rewrite every agent’s `tool_set_ids` at bootstrap.

### 6.3 Filtering

`active_tool_names` (settings or AgentSpec) is an optional allow-list applied
**after** ToolSet expansion. It is for temporary / skill-scoped restriction, not
for defining packs.

## 7. Non-Goals

- Replacing ToolProvider / ToolRegistry.
- Per-tool TOML lists on AgentSpec (sets stay the unit of composition).
- Merging all “coding” tools into one mega-set.
- Making MCP tools appear without explicit set attachment.
- Exposing internal Execution identity through tool schemas.

## 8. Landed Behavior

1. Built-in agent TOML and protocol defaults use the ids in §4.
2. Execution bootstrap registers ToolSet id `todo` (provider `todo`).
3. Attach prefers registered AgentSpec over stale recovery snapshots.
4. Root ensure adds only missing `multi_agent` / `user_interaction`.
5. Catalog tests cover declared expansion and undeclared absence.

## 9. Quick Reference

```text
todo              todo_write, todo_read
workspace         read, bash, edit, write
user_interaction  ask_user, request_user_input
multi_agent       spawn_agent, spawn_agent_detached, send_agent_message,
                  get_agent_status, collect_agent_reports, close_agent, reopen_agent
mcp_<server>      <server-defined>
```
