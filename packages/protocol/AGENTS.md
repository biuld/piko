# piko-protocol

This crate is the foundation of the Piko event-driven protocol architecture.

Do not add orchestrator-owned event sourcing models here. Durable session facts are
stored by piko-hostd as `SessionTreeEntry` JSONL records; piko-orchd emits runtime protocol
events and may keep ephemeral in-memory projections only.

Do not add runtime traits or execution contexts here. Tool provider interfaces,
approval gateways, and tool execution results belong to `piko-orchd::tools`; this
crate should stay as serializable DTOs only.

## Responsibilities
- **Domain Specific Language (DSL)**: Defines serializable protocol structures such as `Command`, `CommandAck`, `Event`, snapshots, messages, session entries, and model config.
- **Single Source of Truth**: Serves as the contract shared by `piko-hostd`, `piko-orchd`, and the generated or hand-maintained TypeScript mirror used by `piko-tui`.
- **Zero Side Effects**: Contains NO execution logic, NO async runtimes, and NO IO operations. Just pure data types and serialization definitions (`serde`).

## Boundaries
- `piko-protocol` does not depend on `piko-orchd` or `piko-hostd`.
- Both `piko-orchd` and `piko-hostd` depend on `piko-protocol`.
- Any new features that require coordination between the TUI, the Host, and the Orchestrator MUST be modeled as events or commands in this crate first.
