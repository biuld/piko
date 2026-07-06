# Named Agents & Discovery

## Overview

Piko currently creates agents dynamically on the fly (`Supervisor::ensure_agent`) using a blank slate if the requested `agent_id` does not exist. However, for a true multi-agent system, the environment needs to define **Named Agents** (e.g., `scout`, `coder`) with specific tools, models, and system prompts. 

This design outlines how Named Agents are defined via TOML, loaded by `hostd`, passed to `orchd`, dynamically exposed to the LLM via tool schemas, and viewed in the TUI.

## 1. Configuration (`hostd`)

Named Agents will be defined in TOML files located in `.piko/agents/*.toml`.

### Schema Map (`AgentSpec`)
```toml
# .piko/agents/scout.toml
name = "Scout"
role = "researcher"
description = "Expert at searching the web and summarizing documentation."
system_prompt = "You are Scout, a specialized web researcher..."
tool_set_ids = ["builtin", "web"]
model = { provider = "anthropic", modelId = "claude-3-5-sonnet-20241022" }
thinking_level = 0
```

### Loading Logic
1. `hostd` includes built-in agent definitions (e.g., `general.toml`, `scout.toml`) compiled into the binary.
2. During startup and `SettingsManager::reload`, `hostd` reads `.piko/agents/*.toml`.
3. Workspace definitions merge with and override built-in definitions by filename/ID.
4. The aggregated `HashMap<String, AgentSpec>` is populated into `OrchdConfig::agents`.

## 2. Agent Discovery (`orchd`)

To allow the LLM to discover available agents without spending a turn calling a `list_agents` tool, we will dynamically inject the available agent names and descriptions into the `spawn` and `spawn_detached` tool schemas.

### Dynamic Tool Schema
In `TaskControlProvider::discover`, the provider will read `OrchdConfig::agents` (via the Supervisor state) and dynamically construct the description for `agent_id`.

Example generated description:
> `"Target agent ID. Available agents: 'scout' (researcher), 'coder' (developer). Or leave empty to use a generic subagent."`

This ensures zero-interaction-overhead for discovery while still making the LLM aware of the specialized personas at its disposal.

## 3. TUI View (`tui` & `protocol`)

The TUI will provide a read-only view of the available agents. Editing will be done manually by users via the TOML files.

1. **Protocol**: Add a `ListAgents` command to `piko_protocol::Command` and a corresponding response containing `Vec<AgentSpec>`.
2. **Hostd**: Implement the command handler in `hostd` to return the current loaded agents.
3. **TUI**: Add a command palette entry (e.g., `/agents`) or a dedicated tab/panel in the UI to display the list of agents, their descriptions, and roles.

## Non-Goals
- TUI-based editing of Agent specs (users will edit TOML files manually).
- Complex DAG-based routing configurations (agents are just specs; routing is handled by the LLM via `spawn`).
