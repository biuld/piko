// orchd — piko orchestrator daemon library
//
// Re-exports all protocol types and orchestrator facade modules.
//
// Architecture (DDD):
//   domain/       — pure rules and types (agents, tasks, tools, events, model)
//   application/  — use case layer (OrchCore facade, agent/task/tool management)
//   runtime/      — actor system and agent loop implementation
//   ports/        — abstract interfaces (ModelGateway, ToolProvider, ApprovalGateway)
//   adapters/     — concrete implementations (tool registry, providers, host events)
//   protocol/     — wire/config DTOs (re-exports piko_protocol)

// The pedantic clippy lints below would require significant refactoring
// (Box'ing enum variants changes serde behaviour, splitting argument lists
// would obscure the 1:1 mapping with TS source). Suppress for now.
#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
pub mod protocol;
pub mod runtime;

// Re-export key types at the crate root for convenience
pub use application::OrchCore;
