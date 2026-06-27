// ---- Model: executor — ModelStepExecutor trait + self-llm adapter ----

use std::future::Future;
use std::pin::Pin;

use tokio_util::sync::CancellationToken;

use crate::protocol::model::ModelCapabilities;
use crate::stream::EventStream;

use super::types::*;

// ---- Trait ----

/// The trait that the orchestrator uses to call LLMs.
///
/// Each call to `execute_step` returns an `EventStream` of `ModelStepEvent`
/// values, with a final `ModelStepResult` available via `.result()`.
pub trait ModelStepExecutor: Send + Sync {
    /// Execute a single model step. Returns a stream of events + deferred final result.
    fn execute_step(
        &self,
        input: ModelStepInput,
        cancel: Option<CancellationToken>,
    ) -> EventStream<ModelStepEvent, ModelStepResult>;

    /// Get model capabilities (tool support, sandbox, etc.).
    fn capabilities(&self) -> ModelCapabilities;

    /// Shutdown the executor gracefully.
    fn shutdown(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    /// Execute a raw stateless chat completion.
    fn llm_call(
        &self,
        model: crate::protocol::messages::Model,
        system_prompt: Option<String>,
        messages: Vec<crate::protocol::messages::Message>,
        settings: crate::protocol::model::ModelRunSettings,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>>;
}

