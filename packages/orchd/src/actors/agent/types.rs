// ---- AgentActor: internal types ----

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;


use crate::protocol::agents::{AgentSpec, AgentTask, HostTaskContext};
use crate::protocol::messages::Message;
use crate::protocol::model::{ModelProviderConfig, ModelRunSettings};
use crate::tools::registry::ToolRegistryImpl;

use llmd::gateway::LlmGateway;

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

// ---- SteerMessage ----

/// A steering message injected into an agent's task by a parent agent or user.
#[derive(Debug, Clone)]
pub struct SteerMessage {
    pub source_task_id: String,
    pub source_agent_id: String,
    pub message: String,
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
    /// Steering queue — messages pushed by parent agents or users via steer_task.
    /// Consumed by the agent loop between steps.
    pub steering_queue: Vec<SteerMessage>,
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
    pub model_executor: Arc<dyn LlmGateway>,
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
    /// Push a steering message into this agent's task queue.
    Steer {
        task_id: String,
        source_task_id: String,
        source_agent_id: String,
        message: String,
    },
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

use serde::{Serialize, Deserialize};

/// Continuation state passed between model steps (extracted from engine_state).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelContinuationState {
    pub version: u32,
    pub kind: String,
    pub counters: ModelRuntimeCounters,
}

/// Runtime counters for model execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRuntimeCounters {
    #[serde(default)]
    pub model_calls: u32,
    #[serde(default)]
    pub tool_calls: u32,
    #[serde(default)]
    pub consecutive_errors: u32,
    #[serde(default)]
    pub started_at: i64,
}

impl ModelContinuationState {
    pub fn extract(raw: Option<&serde_json::Value>) -> Option<Self> {
        raw.and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn ready(counters: ModelRuntimeCounters) -> serde_json::Value {
        serde_json::to_value(Self {
            version: 1,
            kind: "ready".into(),
            counters,
        })
        .unwrap_or_default()
    }
}

impl ModelRuntimeCounters {
    pub fn new() -> Self {
        Self {
            model_calls: 0,
            tool_calls: 0,
            consecutive_errors: 0,
            started_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}

/// Lightweight model reference (not the full pi-ai Model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub provider: String,
}

/// Produce a stable runtime assistant message ID.
pub fn runtime_assistant_message_id(run_id: &str, step_id: &str) -> String {
    format!("{run_id}:{step_id}:assistant")
}
