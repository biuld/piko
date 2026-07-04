// ---- Supervisor struct, state, and basic operations ----

#![allow(dead_code)] // WIP: fields consumed when full Supervisor integration lands

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::model::step::ModelConfig;
use crate::domain::tasks::steering::SteerMessage;
use crate::domain::tasks::task::{
    AgentTaskResult, AgentTaskState, AgentTaskStatus, HostTaskContext, TaskSource,
};
use crate::domain::tools::definition::ToolSet;
use crate::ports::agent_spawner::{AgentReport, AgentSpawner};
use crate::ports::model_gateway::LlmGateway;
use piko_protocol::AgentId;
use piko_protocol::{ServerMessage as Event, TaskEvent};

use super::task_events::RuntimeEventBus;

// ---- AgentHandle — per-agent runtime state ----

pub(crate) struct AgentHandle {
    pub agent_id: AgentId,
    pub parent_agent_id: Option<AgentId>,
    pub cancel: CancellationToken,
    pub steer_tx: tokio::sync::mpsc::UnboundedSender<SteerMessage>,
}

// ---- Shared state ----

pub(crate) struct SupervisorState {
    pub(crate) run_id: String,
    pub(crate) agent_specs: RwLock<HashMap<AgentId, AgentSpec>>,
    pub(crate) dag: RwLock<HashMap<AgentId, Option<AgentId>>>,
    pub(crate) handles: RwLock<HashMap<AgentId, AgentHandle>>,
    pub(crate) runtime_events: RuntimeEventBus,
    pub(crate) registered_task_ids: RwLock<HashSet<String>>,
    pub(crate) task_results: Mutex<HashMap<String, AgentReport>>,
    pub(crate) tasks: RwLock<HashMap<String, AgentTaskState>>,
    pub(crate) steer_tx: RwLock<Option<tokio::sync::mpsc::UnboundedSender<SteerMessage>>>,
    pub(crate) model_executor: Arc<dyn LlmGateway>,
    pub(crate) tool_registry: Arc<ToolRegistryImpl>,
    pub(crate) model_config: Arc<RwLock<Option<ModelConfig>>>,
    pub(crate) default_agent_id: RwLock<String>,
}

// ---- Supervisor ----

pub struct Supervisor {
    pub(crate) state: Arc<SupervisorState>,
}

impl Supervisor {
    pub fn new(
        model_executor: Arc<dyn LlmGateway>,
        tool_registry: Arc<ToolRegistryImpl>,
        model_config: Arc<RwLock<Option<ModelConfig>>>,
    ) -> Self {
        let run_id = format!(
            "run_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        Self {
            state: Arc::new(SupervisorState {
                run_id,
                agent_specs: RwLock::new(HashMap::new()),
                dag: RwLock::new(HashMap::new()),
                handles: RwLock::new(HashMap::new()),
                runtime_events: RuntimeEventBus::new(1024),
                registered_task_ids: RwLock::new(HashSet::new()),
                task_results: Mutex::new(HashMap::new()),
                tasks: RwLock::new(HashMap::new()),
                steer_tx: RwLock::new(None),
                model_executor,
                tool_registry,
                model_config,
                default_agent_id: RwLock::new("main".into()),
            }),
        }
    }

    pub(crate) fn with_state(state: Arc<SupervisorState>) -> Self {
        Self { state }
    }

    // ---- Agent spec management ----

    pub async fn register_agent(&self, spec: AgentSpec) {
        self.state
            .agent_specs
            .write()
            .await
            .insert(spec.id.clone(), spec);
    }

    pub async fn unregister_agent(&self, agent_id: &str) {
        self.state.agent_specs.write().await.remove(agent_id);
    }

    pub async fn agent_spec(&self, agent_id: &str) -> Option<AgentSpec> {
        self.state.agent_specs.read().await.get(agent_id).cloned()
    }

    pub async fn ensure_agent(&self, agent_id: &str) -> AgentSpec {
        let mut specs = self.state.agent_specs.write().await;
        if let Some(spec) = specs.get(agent_id).cloned() {
            spec
        } else {
            let spec = AgentSpec {
                id: agent_id.to_string(),
                name: agent_id.to_string(),
                role: "assistant".into(),
                description: None,
                system_prompt: String::new(),
                model: None,
                tool_set_ids: vec!["builtin".into(), "workspace".into()],
                active_tool_names: None,
                thinking_level: None,
            };
            specs.insert(agent_id.to_string(), spec.clone());
            spec
        }
    }

    // ---- Tool registry ----

    pub async fn register_tool_set(&self, tool_set: ToolSet) {
        self.state.tool_registry.register_tool_set(tool_set).await;
    }

    pub async fn unregister_tool_set(&self, tool_set_id: &str) {
        self.state
            .tool_registry
            .unregister_tool_set(tool_set_id)
            .await;
    }

    // ---- Accessors ----

    pub fn agent_specs(&self) -> &RwLock<HashMap<AgentId, AgentSpec>> {
        &self.state.agent_specs
    }
    pub fn steer_tx(&self) -> &RwLock<Option<tokio::sync::mpsc::UnboundedSender<SteerMessage>>> {
        &self.state.steer_tx
    }
    pub fn model_executor(&self) -> &Arc<dyn LlmGateway> {
        &self.state.model_executor
    }
    pub fn tool_registry(&self) -> &Arc<ToolRegistryImpl> {
        &self.state.tool_registry
    }
    pub fn model_config(&self) -> &Arc<RwLock<Option<ModelConfig>>> {
        &self.state.model_config
    }

    pub fn to_spawner(&self) -> Arc<dyn AgentSpawner> {
        Arc::new(Self {
            state: Arc::clone(&self.state),
        })
    }

    // ---- Convenience: task control (delegate to AgentSpawner) ----

    pub async fn spawn(
        &self,
        agent_id: &str,
        prompt: &str,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> Option<AgentReport> {
        <Self as AgentSpawner>::spawn(self, agent_id, prompt, parent_task_id, host_context).await
    }

    pub async fn spawn_detached(
        &self,
        agent_id: &str,
        prompt: &str,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> String {
        <Self as AgentSpawner>::spawn_detached(self, agent_id, prompt, parent_task_id, host_context)
            .await
    }

    pub async fn poll_task(&self, task_id: &str, timeout_ms: Option<u64>) -> Option<AgentReport> {
        <Self as AgentSpawner>::poll_task(self, task_id, timeout_ms).await
    }

    pub async fn steer_task(&self, task_id: &str, message: &str) -> bool {
        <Self as AgentSpawner>::steer_task(self, task_id, message).await
    }

    pub async fn cancel_task(&self, _task_id: &str, _reason: Option<&str>) {
        // TODO
    }

    // ---- Result recording (called by task drivers and host integration) ----

    pub async fn record_task_result(&self, task_id: &str, report: AgentReport) {
        self.state
            .task_results
            .lock()
            .await
            .insert(task_id.to_string(), report);
    }

    pub(crate) async fn observe_task_event(&self, event: &Event) {
        match event {
            Event::Task(TaskEvent::Created {
                task_id,
                agent_id,
                parent_task_id,
                source_agent_id,
                prompt,
                ..
            }) => {
                let source = match (source_agent_id, parent_task_id) {
                    (Some(agent_id), Some(task_id)) => TaskSource::Agent {
                        agent_id: agent_id.clone(),
                        task_id: task_id.clone(),
                    },
                    _ => TaskSource::User,
                };
                self.state.tasks.write().await.insert(
                    task_id.clone(),
                    AgentTaskState {
                        id: task_id.clone(),
                        target_agent_id: agent_id.clone(),
                        prompt: prompt.clone(),
                        source,
                        status: AgentTaskStatus::Queued,
                        priority: 0,
                        parent_task_id: parent_task_id.clone(),
                        result: None,
                        error: None,
                    },
                );
            }
            Event::Task(TaskEvent::Started { task_id, .. }) => {
                if let Some(task) = self.state.tasks.write().await.get_mut(task_id) {
                    task.status = AgentTaskStatus::Running;
                }
            }
            Event::Task(TaskEvent::Completed {
                task_id, summary, ..
            }) => {
                if let Some(task) = self.state.tasks.write().await.get_mut(task_id) {
                    task.status = AgentTaskStatus::Completed;
                    task.result = Some(AgentTaskResult {
                        summary: summary.clone(),
                        artifacts: None,
                    });
                    task.error = None;
                }
            }
            Event::Task(TaskEvent::Failed { task_id, error, .. }) => {
                if let Some(task) = self.state.tasks.write().await.get_mut(task_id) {
                    task.status = AgentTaskStatus::Failed;
                    task.error = Some(error.clone());
                }
            }
            Event::Task(TaskEvent::Cancelled { task_id, .. }) => {
                if let Some(task) = self.state.tasks.write().await.get_mut(task_id) {
                    task.status = AgentTaskStatus::Cancelled;
                }
            }
            _ => {}
        }
    }
}
