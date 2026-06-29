// ---- Adapters: model — LLM gateway adapter ----
//
// The model gateway is implemented by the llmd crate.
// This module re-exports it and provides any orchd-specific wrappers.

pub use llmd::gateway::LlmGateway;
