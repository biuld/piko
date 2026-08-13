# Named Agents & Discovery

## Overview

Piko models named agents as static `AgentSpec` templates. The future multi-agent
identity boundary follows the PRD-first runtime model (see the
[feature index](../../../../docs/features/README.md) and the
[codex-rs Agent Core Digest](../../../../docs/codex-agent-core-digest.md)).

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

Session **runtime agents** are the primary UX: `/agents` (and F4) open the
Select / ComposerBand picker over the current session’s agent instances so
the user can switch the viewed agent.

Static **agent specs** remain host-owned templates (`AgentSpecList`). Spec discovery
for the model is injected into spawn tool schemas (see §2); the TUI no longer
exposes a dedicated agent-spec panel via `/agents`.

1. **Protocol**: `AgentSpecList` returns `Vec<AgentSpec>` (templates).
2. **Hostd**: The command handler returns the loaded installed/workspace spec set.
3. **TUI**: Runtime instances use `SurfaceId::Agents` (viewed-agent switch).

## Non-Goals
- TUI-based editing of agent specs.
- Complex DAG-based routing configurations. Agent specs are templates; runtime parent/child relationships are task DAG edges created by `spawn`.
