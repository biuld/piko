// ---- Runtime: agent messages and internal state ----
//
// These types are runtime implementation details of the AgentActor.
// Domain-level types (AgentStatus, ModelConfig, SteerMessage, etc.)
// have been moved to the domain layer.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::AgentStatus;
use crate::domain::agents::spec::AgentSpec;
use piko_protocol::Event;
use tokio::sync::mpsc;
use crate::domain::model::step::ModelConfig;
use crate::domain::model::transcript::Message;
use crate::domain::tasks::steering::SteerMessage;
use crate::domain::tasks::task::{AgentArtifact, AgentTask, HostTaskContext};
use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::ports::model_gateway::LlmGateway;

// ---- AgentRuntimeState ----

/// Mutable state owned by the AgentActor, protected by Mutex.
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
    /// Steering queue — messages pushed by parent agents or users via steer_task.
    /// Consumed by the agent loop between steps.
    pub steering_queue: Vec<SteerMessage>,
}

// ---- Per-run worker state ----

/// Mutable state for a single agent run (one task execution).
pub struct AgentWorkerState {
    pub transcript: Vec<Message>,
    pub step_count: u32,
    pub next_message_index: u32,
    pub message_index_by_id: HashMap<String, u32>,
    pub event_seq: u64,
    pub engine_state: Option<serde_json::Value>,
}

// ---- AgentActorDeps ----

/// Dependencies injected into the AgentActor.
#[derive(Clone)]
pub struct AgentActorDeps {
    pub model_executor: Arc<dyn LlmGateway>,
    pub model_config: Option<ModelConfig>,
    pub tool_registry: Arc<ToolRegistryImpl>,
    /// Sender for host-facing events. Cloned from OrchCore.event_tx.
    /// Written per-run via `begin_run()`, read by agents during execution.
    pub event_tx: Arc<tokio::sync::RwLock<Option<mpsc::UnboundedSender<Event>>>>,
}

// ---- Agent messages ----
// Typed messages, not serialized. tokio-actors uses direct typed dispatch.

/// Messages that the AgentActor can receive.
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
    /// Push a steering message into this agent's task queue.
    Steer {
        task_id: String,
        source_task_id: String,
        source_agent_id: String,
        message: String,
    },
}

// ---- Extended task result ----

/// Extended task result returned from the agent loop.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentTaskResultExt {
    pub summary: String,
    pub messages: Vec<Message>,
    pub total_steps: u32,
    pub final_status: String,
    pub artifacts: Option<Vec<AgentArtifact>>,
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

/// Outcome of processing a single model step.
pub enum StepOutcome {
    Continue,
    Terminal { result: AgentTaskResultExt },
}

// ---- Helpers ----

/// Produce a stable runtime assistant message ID.
pub fn runtime_assistant_message_id(run_id: &str, step_id: &str) -> String {
    format!("{run_id}:{step_id}:assistant")
}
