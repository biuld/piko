// ---- Model: LLM execution subsystem ----
//
// Provides the `ModelStepExecutor` trait and a `self-llm` based implementation.

pub mod executor;
pub mod types;

// Re-exports
pub use executor::{ModelStepExecutor, SelfLlmExecutor};
pub use types::{
    ModelContinuationState, ModelRuntimeCounters, ModelSpec, ModelStepEvent, ModelStepInput,
    ModelStepResult, TranscriptDelta, runtime_assistant_message_id,
};
