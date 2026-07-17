# piko

piko is a Rust-based coding agent harness with a decoupled **hostd + orchd** architecture. It separates state management (sessions, prompts, settings, and compaction) in the Host daemon from transient agent execution in the stream-driven Orchestrator, using a Ratatui-based terminal UI client connected over JSON-lines stdio.

---

## Architecture

```mermaid
graph TD
    subgraph Client Layer
        TUI["piko-tui (Ratatui Terminal UI)"]
    end

    subgraph Host Layer
        Hostd["piko-hostd (Host Daemon)"]
    end

    subgraph Orchestration Layer
        OrchdApi["piko-orchd-api (Port & DTO Traits)"]
        Orchd["piko-orchd (Agent Runtime)"]
    end

    subgraph Core Services & Sandbox
        LLMd["piko-llmd (LLM Gateway)"]
        Sandbox["piko-sandbox (Process Sandbox)"]
    end

    subgraph Shared Foundations
        Comms["piko-comms (Typed Channels)"]
        Protocol["piko-protocol (DTOs & DSL)"]
    end

    %% Flow of control
    TUI <-->|JSON-lines over stdio| Hostd
    Hostd -.->|implements ports| OrchdApi
    Orchd -.->|implements traits| OrchdApi
    Hostd -->|drives API| Orchd
    Orchd --> LLMd
    Hostd --> LLMd
    Orchd --> Sandbox

    %% Core dependencies
    TUI -.-> Comms
    Hostd -.-> Comms
    Orchd -.-> Comms

    TUI -.-> Protocol
    Hostd -.-> Protocol
    Orchd -.-> Protocol
    LLMd -.-> Protocol
```

### Crates & Project Layout

| Crate | Directory | Type | Description |
|---|---|---|---|
| `piko-tui` | `packages/tui` | binary (`piko-tui`) | Terminal UI built with Ratatui (Timeline, Session view, Command dispatch, Keymap) |
| `piko-hostd` | `packages/hostd` | bin + lib (`piko-hostd`) | State authority: manages sessions, configuration, credentials, prompt assembly, and turn compaction |
| `piko-orchd-api` | `packages/orchd-api` | library | Public traits, interfaces, and ports defining the Orchestrator contract |
| `piko-orchd` | `packages/orchd` | library | Transient agent execution runtime (AgentActor & ExecutionActor scheduling) |
| `piko-llmd` | `packages/llmd` | library | LLM provider registry, token/cost middleware, and OAuth provider gateway |
| `piko-comms` | `packages/comms` | bin + lib | Bounded, contract-enforced async channel topology ensuring design-compliant backpressure |
| `piko-protocol` | `packages/protocol` | library | Shared ubiquitous DTOs, wire formats, commands, and events |
| `piko-sandbox` | `packages/sandbox` | library | Fail-closed process and filesystem sandbox for sandboxed CLI execution |

---

## Core Design Principles

- **Host-Authoritative State:** `hostd` owns all user-visible state (sessions, prompts, settings, compaction). `orchd` is transient: it receives input, runs agent loops, and writes executions back to hostd via durability ports.
- **Clean Interface Boundary (Ports & Adapters):** `hostd` and `orchd` communicate strictly through `orchd-api`. `orchd` defines interfaces (ports) for storage, LLM, and tool approvals, allowing developers to mock components cleanly.
- **Contract-Enforced Channels:** `piko-comms` replaces ad-hoc Tokio channels. All asynchronous channels conform to predefined contracts (e.g. Mailbox, Reply, LatestState, Broadcast, ThreadBridge).
- **Stream-Driven Execution:** Step mutations, tool outputs, and LLM completions are compiled into a unified reactive stream (`Stream<Item = OrchEvent>`), avoiding raw spawns and lock contention.

---

## Quick Start

### Build

Ensure you have a stable [Rust toolchain](https://rustup.rs) installed:

```bash
# Clone & build
git clone <repo-url> piko
cd piko
cargo build --release
```

### Run

Set your LLM provider API key and start the terminal user interface:

```bash
export ANTHROPIC_API_KEY=sk-ant-...

# Start a new session
cargo run -p piko-tui

# Continue the most recent session
cargo run -p piko-tui -- -c

# Run with a specific model and thinking level
cargo run -p piko-tui -- -m claude-3-5-sonnet-20241022 --thinking-level medium
```

---

## CLI Reference

```text
piko-tui [options]

  -c, --continue             Continue the most recent session
  --session <id>             Open a specific session
  --name <name>              Set session name (only for new sessions)
  -m, --model <id>           Override the Model ID
  -p, --provider <name>      Override the Provider (e.g., anthropic, openai)
  -k, --api-key <key>        Provide API key (forwarded directly to hostd)
  --thinking-level <level>   Specify thinking level (off | low | medium | high)
  --no-tools                 Disable all tools for this session
  --hostd <path>             Override the hostd executable path
  --hostd-arg <arg>          Extra hostd argument (can be repeated)
  --log-file <path>          Path to hostd log file
  --log-level <level>        Hostd log level filter
  --debug                    Enable debug logging
  --no-log                   Disable hostd logging
  -h, --help                 Show help message
```

---

## Development

### Workspace Commands

Run check, formatting, and lint rules:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

### Testing

Run the entire test suite or run tests for a specific crate:

```bash
# Test the entire workspace
cargo test --workspace

# Per-crate testing
cargo test -p piko-hostd
cargo test -p piko-orchd
cargo test -p piko-orchd-api
cargo test -p piko-tui
cargo test -p piko-llmd
cargo test -p piko-comms
cargo test -p piko-protocol
cargo test -p piko-sandbox
```

### Communication Topology

The communication channels are checked for drift as part of `cargo test` using the checked-in topology definition at `docs/generated/communication-topology.md`. To update or manually check the topology:

```bash
# Check for topology definition drift
cargo run -p piko-comms --bin piko-comms-topology -- --check docs/generated/communication-topology.md docs/generated/communication-topology.json

# Regenerate topology definitions
cargo run -p piko-comms --bin piko-comms-topology -- docs/generated/communication-topology.md docs/generated/communication-topology.json
```

---

## License

This project is licensed under the **Apache License, Version 2.0**. See the [LICENSE](LICENSE) file for details.
