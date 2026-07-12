// ---- Domain: pure rules and types ----
//!
//! Domain layer contains pure types, rules, and abstractions.
//! It should NOT depend on tokio actors, LLM gateways, tool registries,
//! or any runtime infrastructure.

mod event;
pub mod model;
pub mod tools;
pub mod transcript;

pub use event::RealtimeFrame;
