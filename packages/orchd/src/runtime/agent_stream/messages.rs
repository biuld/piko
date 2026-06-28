// ---- Runtime: agent execution state ----
//
// These types are runtime implementation details of agent execution.
// Domain-level types (AgentStatus, ModelConfig, SteerMessage, etc.)
// have been moved to the domain layer.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::agents::AgentStatus;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::events::event::Event;
use crate::domain::model::step::ModelConfig;
use crate::domain::model::transcript::Message;
use crate::domain::tasks::steering::SteerMessage;
use crate::domain::tasks::task::{AgentArtifact, HostTaskContext};
use crate::ports::model_gateway::LlmGateway;


// ---- AgentRuntimeState ----

/// Mutable state owned by an agent run, protected by Mutex.
#[allow(dead_code)]
pub(crate) struct AgentRuntimeState {
    pub spec: AgentSpec,
    pub status: AgentStatus,
    pub current_task_id: Option<String>,
    pub current_parent_task_id: Option<String>,
    pub current_host_context: Option<HostTaskContext>,
    pub current_run_token: Option<u64>,
    pub next_run_token: u64,
    pub terminal_committed: bool,
    pub abort_token: Option<CancellationToken>,
    /// Steering queue — messages pushed by parent agents or users via steer_task.
    /// Consumed by the agent loop between steps.
    pub steering_queue: Vec<SteerMessage>,
}

// ---- Per-run worker state ----

/// Mutable state for a single agent run (one task execution).
#[allow(dead_code)]
pub(crate) struct AgentWorkerState {
    pub transcript: Vec<Message>,
    pub step_count: u32,
    pub next_message_index: u32,
    pub message_index_by_id: HashMap<String, u32>,
    pub event_seq: u64,
    pub engine_state: Option<serde_json::Value>,
}

// ---- AgentRunDeps ----

/// Dependencies injected into an agent run.
#[derive(Clone)]
pub(crate) struct AgentRunDeps {
    pub model_executor: Arc<dyn LlmGateway>,
    pub model_config: Option<ModelConfig>,
    pub tool_registry: Arc<ToolRegistryImpl>,
    /// Optional sender for child agent events. Set for child tasks
    /// so events flow to the parent's event stream.
    pub event_tx: Option<mpsc::UnboundedSender<Event>>,
}

// ---- Extended task result ----

/// Extended task result returned from the agent loop.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct AgentTaskResultExt {
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
pub(crate) enum StepOutcome {
    Continue {
        events: Vec<crate::domain::events::event::Event>,
    },
    Terminal {
        result: AgentTaskResultExt,
    },
}

// ---- Helpers ----

/// Produce a stable runtime assistant message ID.
pub fn runtime_assistant_message_id(run_id: &str, step_id: &str) -> String {
    format!("{run_id}:{step_id}:assistant")
}
