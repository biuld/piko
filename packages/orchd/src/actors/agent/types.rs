// ---- AgentActor: internal types ----

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::model::types::ModelSpec;
use crate::protocol::agents::{AgentSpec, AgentTask, HostTaskContext};
use crate::protocol::messages::Message;
use crate::protocol::model::{ModelProviderConfig, ModelRunSettings};
use crate::tools::registry::ToolRegistryImpl;

use super::step_runner::ModelStepExecutor;

// ---- AgentStatus ----

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Running,
    Cancelling,
}

// ---- ModelConfig ----

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub model: ModelSpec,
    pub provider: ModelProviderConfig,
    pub settings: ModelRunSettings,
}

// ---- AgentRuntimeState ----

pub struct AgentRuntimeState {
    pub spec: AgentSpec,
    pub status: AgentStatus,
    pub current_task_id: Option<String>,
    pub current_parent_task_id: Option<String>,
    pub current_host_context: Option<HostTaskContext>,
    pub current_run_token: Option<u64>,
    pub next_run_token: u64,
    /// For Dispatch: the oneshot sender to reply with the final result.
    /// Stored here so the engine loop can use it when done.
    pub pending_reply_tx: Option<oneshot::Sender<AgentTaskResultExt>>,
    pub terminal_committed: bool,
    pub abort_token: Option<CancellationToken>,
}

// ---- Per-run worker state ----

pub struct AgentWorkerState {
    pub transcript: Vec<Message>,
    pub step_count: u32,
    pub next_message_index: u32,
    pub message_index_by_id: HashMap<String, u32>,
    pub event_seq: u64,
    pub engine_state: Option<serde_json::Value>,
}

// ---- AgentActorDeps ----

#[derive(Clone)]
pub struct AgentActorDeps {
    pub model_executor: Arc<dyn ModelStepExecutor>,
    pub model_config: Option<ModelConfig>,
    pub tool_registry: Arc<ToolRegistryImpl>,
    pub emit_fn: Arc<
        dyn Fn(
                String,
                serde_json::Value,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync,
    >,
}

// ---- Agent messages ----
// Typed messages, not serialized. tokio-actors uses direct typed dispatch.

#[derive(Debug)]
pub enum AgentMsg {
    /// Dispatch a task. Contains a oneshot sender for the deferred result.
    Dispatch {
        task: AgentTask,
        reply_tx: oneshot::Sender<AgentTaskResultExt>,
    },
    /// Engine loop finished successfully (sent from spawned task back to self).
    RunnerFinished {
        task_id: String,
        token: u64,
        result: AgentTaskResultExt,
    },
    /// Engine loop failed.
    RunnerFailed {
        task_id: String,
        token: u64,
        error: String,
    },
    /// Cancel the running task.
    Cancel {
        task_id: String,
        reason: Option<String>,
    },
    /// Health-check wake.
    Wake,
    /// Update model config at runtime.
    SetModelConfig { config: ModelConfig },
}

// ---- Extended task result ----

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentTaskResultExt {
    pub summary: String,
    pub messages: Vec<Message>,
    pub total_steps: u32,
    pub final_status: String,
    pub artifacts: Option<Vec<crate::protocol::agents::AgentArtifact>>,
}

impl Default for AgentTaskResultExt {
    fn default() -> Self {
        Self {
            summary: String::new(),
            messages: vec![],
            total_steps: 0,
            final_status: "completed".into(),
            artifacts: None,
        }
    }
}

// ---- Step outcome ----

pub enum StepOutcome {
    Continue,
    Terminal { result: AgentTaskResultExt },
}
