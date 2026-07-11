// ---- Supervisor struct, state, and basic operations ----

#![allow(dead_code)] // Runtime fields are consumed by control and graph APIs as features compose.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::model::step::ModelConfig;
use crate::domain::tasks::task::HostTaskContext;
use crate::domain::tools::definition::ToolSet;
use crate::ports::agent_spawner::{AgentReport, AgentSpawner};
use crate::ports::model_gateway::LlmGateway;
use crate::runtime::types::TaskMailboxMessage;
use piko_protocol::agent_runtime::TaskControlRequest;
use piko_protocol::AgentId;

use super::task_registry::TaskRegistry;

// ---- Shared state ----

pub(crate) struct SupervisorState {
    pub(crate) run_id: String,
    pub(crate) agent_specs: RwLock<HashMap<AgentId, AgentSpec>>,
    pub(crate) registry: Arc<TaskRegistry>,
    pub(crate) task_event_tx: tokio::sync::mpsc::UnboundedSender<piko_protocol::TaskEvent>,
    task_event_rx:
        std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<piko_protocol::TaskEvent>>>,
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
        let registry = Arc::new(TaskRegistry::new());
        let (task_event_tx, task_event_rx) = tokio::sync::mpsc::unbounded_channel();
        let state = Arc::new(SupervisorState {
            run_id,
            agent_specs: RwLock::new(HashMap::new()),
            registry: Arc::clone(&registry),
            task_event_tx,
            task_event_rx: std::sync::Mutex::new(Some(task_event_rx)),
            model_executor,
            tool_registry,
            model_config,
            default_agent_id: RwLock::new("main".into()),
        });
        Self { state }
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

    pub(crate) async fn register_task_runtime(
        &self,
        task: &crate::domain::tasks::task::AgentTask,
        agent_id: &str,
        cancel: CancellationToken,
        control_tx: tokio::sync::mpsc::UnboundedSender<TaskMailboxMessage>,
    ) -> String {
        self.state
            .registry
            .register_runtime(task, agent_id, cancel, control_tx)
            .await
    }

    pub(crate) async fn cleanup_task_runtime(&self, task_id: &str) {
        self.state.registry.cleanup_runtime(task_id).await;
    }

    // ---- Convenience: task control (delegate to AgentSpawner) ----

    pub async fn spawn(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> Option<AgentReport> {
        <Self as AgentSpawner>::spawn(
            self,
            agent_id,
            prompt,
            source_agent_id,
            parent_task_id,
            host_context,
            senders,
        )
        .await
    }

    pub async fn spawn_detached(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> String {
        <Self as AgentSpawner>::spawn_detached(
            self,
            agent_id,
            prompt,
            source_agent_id,
            parent_task_id,
            host_context,
            senders,
        )
        .await
    }

    pub async fn poll_task(&self, task_id: &str) -> Option<AgentReport> {
        <Self as AgentSpawner>::poll_task(self, task_id).await
    }

    pub async fn steer_task(&self, task_id: &str, message: &str) -> bool {
        <Self as AgentSpawner>::steer_task(self, task_id, message, None, None, None).await
    }

    pub async fn cancel_task(&self, task_id: &str, _reason: Option<&str>) {
        if let Some(handle) = self.state.registry.handle(task_id).await {
            handle.cancel.cancel();
        }
    }

    pub async fn close_task(&self, task_id: &str) -> bool {
        if let Some(handle) = self.state.registry.handle(task_id).await {
            handle
                .control_tx
                .send(TaskMailboxMessage::Control(TaskControlRequest::Close {
                    request_id: format!("req_{}", uuid::Uuid::new_v4()),
                    task_id: task_id.to_string(),
                }))
                .is_ok()
        } else {
            false
        }
    }

    pub async fn reopen_task(&self, task_id: &str) -> bool {
        if let Some(handle) = self.state.registry.handle(task_id).await {
            handle
                .control_tx
                .send(TaskMailboxMessage::Control(TaskControlRequest::Reopen {
                    request_id: format!("req_{}", uuid::Uuid::new_v4()),
                    task_id: task_id.to_string(),
                }))
                .is_ok()
        } else {
            false
        }
    }

    // ---- Result recording (called by task drivers and host integration) ----

    pub async fn record_task_result(&self, task_id: &str, report: AgentReport) {
        self.state
            .registry
            .record_task_result(task_id, report)
            .await;
    }
}

impl SupervisorState {
    pub(crate) fn ensure_task_event_projector(&self) {
        let Some(mut events) = self
            .task_event_rx
            .lock()
            .expect("task event receiver lock poisoned")
            .take()
        else {
            return;
        };
        let registry = Arc::clone(&self.registry);
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                registry.apply_task_event(&event).await;
            }
        });
    }
}
