# Named Agents & Discovery

## Overview

Piko models named agents as static `AgentSpec` templates. The future multi-agent
identity boundary is defined by the
[Single-Agent Runtime Model](../../../../docs/single-agent-runtime-model.md#15-multi-agent-extension-boundary).

This TUI design document only covers how template discovery is exposed to users.

## 1. Configuration Source

Agent templates are configured with TOML and loaded by hostd.

## 2. Agent Discovery (`orchd`)

To allow the LLM to discover available agent specs without spending a turn calling a `list_agents` tool, orchd dynamically injects the available agent names and descriptions into the `spawn` and `spawn_detached` tool schemas.

### Dynamic Tool Schema
In `TaskControlProvider::discover`, the provider reads `OrchdConfig::agents` and dynamically constructs the description for `agent_id`.

Example generated description:
> `"Target agent template ID. Available agent templates: 'scout' (researcher), 'coder' (developer). Omit to use 'general'."`

This gives the LLM the delegated-task template IDs at tool-call time. `main` is the fixed root-turn template and is not advertised as a delegated-task option. Each tool call that spawns an agent creates a distinct runtime task instance with its own `task_id`.

## 3. TUI View (`tui` & `protocol`)

The TUI provides a read-only view of available agent specs. Runtime task instances are shown separately in the agent panel through `AgentList`, where rows are keyed by `task_id` and labeled by `agent_id` / spec name.

1. **Protocol**: `AgentSpecList` returns `Vec<AgentSpec>`.
2. **Hostd**: The command handler returns the loaded built-in/workspace spec set.
3. **TUI**: `/agents` or a dedicated panel displays agent specs, descriptions, roles, and configured tools.

## Non-Goals
- TUI-based editing of agent specs.
- Complex DAG-based routing configurations. Agent specs are templates; runtime parent/child relationships are task DAG edges created by `spawn`.
