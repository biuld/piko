# piko-protocol

This crate is the foundation of the Piko event-driven protocol architecture.

Do not add orchestrator-owned event sourcing models here. Durable session facts are
stored by hostd as `SessionTreeEntry` JSONL records; orchd emits runtime protocol
events and may keep ephemeral in-memory projections only.

Do not add runtime traits or execution contexts here. Tool provider interfaces,
approval gateways, and tool execution results belong to `orchd::tools`; this
crate should stay as serializable DTOs only.

## Responsibilities
- **Domain Specific Language (DSL)**: Defines all pure event structures (`HostEvent`), commands (`HostCommand`), and entities (`AgentSpec`, `Message`, `ToolCall`).
- **Single Source of Truth**: Serves as the ultimate contract between `hostd` (State / Session / IO) and `orchd` (Agent runtime).
- **Zero Side Effects**: Contains NO execution logic, NO async runtimes, and NO IO operations. Just pure data types and serialization definitions (`serde`).

## Boundaries
- `piko-protocol` does not depend on `orchd` or `hostd`.
- Both `orchd` and `hostd` depend on `piko-protocol`.
- Any new features that require coordination between the TUI, the Host, and the Orchestrator MUST be modeled as events or commands in this crate first.
