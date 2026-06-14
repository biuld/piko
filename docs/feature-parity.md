# piko - Feature Parity & Implementation Status

> **Status Marks:** ✅ Completed · 🟡 Partial · 🔶 Deferred / Out of Scope

This document tracks the feature parity of **piko** compared to the reference implementation in `pi-mono`. 
It covers the actor-based Orchestrator, the stateful Host Runtime, and the solid Terminal UI, highlighting the transition from the monolithic design of `pi-mono` to piko's clean **Host + Orchestrator** split.

---

## Architecture Overview

```text
               ┌──────────────┐
               │     cli      │
               └──────┬───────┘
                      │
              ┌───────▼───────┐
              │   host-tui    │
              └───────┬───────┘
                      │
             ┌────────▼────────┐
             │  host-runtime   │
             └────┬────────┬───┘
                  │        │
      ┌───────────▼──┐  ┌──▼───────────┐
      │ orchestrator │  │   session    │
      └──────────────┘  └──────────────┘
```

- **`orchestrator-protocol`**: Pure TypeScript interface definitions. Contains no runtime dependencies beyond `pi-ai` types. Holds types for `AgentSpec`, `ToolSet`, `HostEvent`, `ApprovalGateway`, and `OrchState`.
- **`orchestrator`**: Actor-first runtime utilizing a lightweight `ActorSystem` kernel. Manages concurrent execution, tool policies, model step delegation, and actor life cycle (`MainActor`, `AgentActor`, `ToolActor`, `StateActor`).
- **`session`**: JSONL-based storage layer matching the `pi-mono` schema for transcripts, branch forks, and metadata compaction.
- **`host-runtime`**: The stateful engine controller. Manages session lifecycles, user-defined settings (`SettingsManager`), model resolution/credentials (`ModelRegistry`, `AuthStorage`), MCP servers, and prompt/skill compilation.
- **`host-tui`**: Autocomplete-equipped SolidJS terminal UI. Manages z-order surfaces, commands, keymaps, timelines, and themes.
- **`cli`**: Entrypoint binary that wires up auth/settings layers and starts the TUI.

---

## Core Feature Parity Checklist

### 1. Agent Loop & Turn State
- **Dynamic Turn Preparation**: Turn settings (models, active tools, system prompt overrides) are dynamically compiled into a unified, immutable `TurnState` snapshot per turn. (Status: ✅ Completed)
- **Rich Lifecycle Events**: The scheduler emits events for turn boundaries, message status, tool execution phases (start, update, end, approval), and synthetic failure messages. (Status: ✅ Completed)
- **Active Tools Policy**: Support for restricting the tools an agent can call on any given turn (e.g., via skill metadata or explicit runtime selection), preserved across session restore. (Status: ✅ Completed)
- **Queue Semantics**: Differentiates between:
  - *Steering*: Injected mid-turn (rejected if idle).
  - *Follow-ups*: Runs sequentially after current turn finishes (rejected if idle).
  - *Next-turn*: Standard prompt queued for the next user turn. (Status: ✅ Completed)

### 2. Tool Execution Semantics
- **Parallel & Sequential Branching**: Executes tool calls in parallel by default, but defaults to sequential if specified in settings or if *any* requested tool dictates sequential execution. (Status: ✅ Completed)
- **Order Preservation**: Standardizes result sequence in session transcripts regardless of whether tool execution was completed concurrently or sequentially. (Status: ✅ Completed)
- **Approval Gateway**: Prompts for approval on dangerous tools, pausing the orchestration flow and correctly resuming execution after acceptance. (Status: ✅ Completed)

### 3. Session & Compaction
- **Branch Navigation**: Full session tree traversal via clone/fork/resume, with auto-generated branch summaries injected on divergence. (Status: ✅ Completed)
- **Crash Resilience**: Incremental save checkpoints and per-message flush policies ensure minimal data loss if execution crashes. (Status: ✅ Completed)
- **Context Compaction**: Automated message compaction based on token limits, summarizing historical context without losing system instructions or active settings. (Status: ✅ Completed)

### 4. Integration & Security
- **MCP (Model Context Protocol)**: Dynamic registration of MCP tools via `McpServerManager`. Exposes namespaced MCP servers to orchestrator toolsets. (Status: ✅ Completed)
- **OAuth (RFC 8628)**: Solid device-code flow with WSL/VM time-drift warnings, proper slow-down backoffs, and cleanup via `AbortSignal` when cancelled. (Status: ✅ Completed)
- **Auth & Settings Layers**: Layered overrides (defaults → global setting → project configuration → CLI flags) for API keys and engine defaults. (Status: ✅ Completed)

---

## Intentionally Deferred / Non-Goal Features

To keep the boundaries of piko's **Host + stateless Orchestrator** architecture clean, certain features from `pi-mono` are deferred or not planned for 1:1 parity:

### 1. CLI Execution Modes
- **RPC Server & Non-Interactive Print**: Pi's standard `--print` non-interactive mode, structured `--mode json|rpc` formats, and RPC daemon features are deferred. Piko CLI acts strictly as an interactive TUI launcher for now. (Status: 🔶 Deferred)
- **Advanced CLI Flags**: Exclude/include-tools flags, modelCycling scopes, and fuzzy pattern matching for provider shortcodes. (Status: 🔶 Deferred)

### 2. Extensibility & Hooks
- **Monolithic Hook Contracts**: Pi's internal hook events (e.g., intercepting provider payloads, custom transform filters) are not supported. (Status: 🔶 Not Planned)
- **Package Resource Loading**: Global discovery and auto-loading of package-installed skills, themes, or templates. Only local project resources are supported. (Status: 🔶 Not Planned)

### 3. TUI Minor Items
- **Slash Commands**: `/share` and `/changelog` commands are defined in menus but not implemented. (Status: 🔶 Deferred)
- **Settings Screen**: The `/settings` dashboard is functional but features a simpler set of configuration toggles than Pi's exhaustive layout. (Status: 🟡 Partial)

---

## Quality Gates

- **Static Analysis**: Project passes strict Biome linting and formatting (`bun run fmt`).
- **Compilation**: Full TypeScript compilation succeeds with project references.
- **Unit & Integration Tests**: Test suite contains **440+ tests** (including virtualized HOME directory sandbox testing), executing successfully via `bun run test`.
