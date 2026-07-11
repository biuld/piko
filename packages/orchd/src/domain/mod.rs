// ---- Domain: pure rules and types ----
//
// Domain layer contains pure types, rules, and abstractions.
// It should NOT depend on tokio actors, LLM gateways, tool registries,
// or any runtime infrastructure.
//
// Dependencies flow inward: domain ← ports ← adapters ← application ← runtime

pub mod agents;
mod event;
pub mod model;
pub mod tasks;
pub mod tools;
pub mod transcript;
pub mod work;

pub use event::RealtimeFrame;
