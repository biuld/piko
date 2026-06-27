# piko-protocol

The core Domain-Specific Language (DSL) and event definitions for Piko.

It defines the command/event DTOs that connect hostd, orchd, and host-tui.
Durable session state is owned by hostd's `SessionTreeEntry` JSONL log; orchd
events are runtime notifications, not a second persistent event-sourcing layer.
Runtime extension traits such as tool providers and approval gateways live in
`orchd::tools`, not in this crate.
By isolating these types into a standalone crate, Piko ensures that both the orchestrator (`orchd`) and the host (`hostd`) share a ubiquitous language without circular dependencies or boundary erosion.
