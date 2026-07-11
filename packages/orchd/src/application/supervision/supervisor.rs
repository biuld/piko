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
use crate::domain::work::TaskReport;
use crate::ports::model_gateway::LlmGateway;
use crate::ports::task_control::TaskControlPort;
use crate::runtime::events::InternalLifecycleObserver;
use crate::runtime::task::mailbox::TaskMailboxMessage;
use piko_protocol::AgentId;

use super::registry::TaskRegistry;

// ---- Shared state ----

pub(crate) struct SupervisorState {
    pub(crate) run_id: String,
    pub(crate) agent_specs: RwLock<HashMap<AgentId, AgentSpec>>,
    pub(crate) registry: Arc<TaskRegistry>,
    pub(crate) lifecycle_observer: InternalLifecycleObserver,
    pub(crate) model_executor: Arc<dyn LlmGateway>,
    pub(crate) tool_registry: Arc<ToolRegistryImpl>,
    pub(crate) model_config: Arc<RwLock<Option<ModelConfig>>>,
    pub(crate) default_agent_id: RwLock<String>,
    pub(crate) persist_sink: RwLock<Option<Arc<dyn orchd_api::PersistSink>>>,
    pub(crate) session_hubs: RwLock<HashMap<String, Arc<crate::runtime::events::SessionOutputHub>>>,
    pub(crate) task_control: RwLock<Option<Arc<dyn TaskControlPort>>>,
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
        let lifecycle_observer = InternalLifecycleObserver::new(Arc::clone(&registry));
        let state = Arc::new(SupervisorState {
            run_id,
            agent_specs: RwLock::new(HashMap::new()),
            registry,
            lifecycle_observer,
            model_executor,
            tool_registry,
            model_config,
            default_agent_id: RwLock::new("main".into()),
            persist_sink: RwLock::new(None),
            session_hubs: RwLock::new(HashMap::new()),
            task_control: RwLock::new(None),
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

    pub async fn set_task_control(&self, port: Arc<dyn TaskControlPort>) {
        *self.state.task_control.write().await = Some(port);
    }

    pub async fn task_control(&self) -> Option<Arc<dyn TaskControlPort>> {
        self.state.task_control.read().await.clone()
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

    pub async fn set_persist_sink(&self, sink: Arc<dyn orchd_api::PersistSink>) {
        *self.state.persist_sink.write().await = Some(sink);
    }

    pub async fn persist_sink(&self) -> Option<Arc<dyn orchd_api::PersistSink>> {
        self.state.persist_sink.read().await.clone()
    }

    pub async fn session_hub(
        &self,
        session_id: &str,
    ) -> Arc<crate::runtime::events::SessionOutputHub> {
        let mut hubs = self.state.session_hubs.write().await;
        hubs.entry(session_id.to_string())
            .or_insert_with(|| {
                Arc::new(crate::runtime::events::SessionOutputHub::new(
                    session_id.to_string(),
                    self.state.run_id.clone(),
                    256,
                ))
            })
            .clone()
    }

    // ---- Convenience: task control ----

    pub async fn spawn(
        &self,
        agent_id: &str,
        prompt: &str,
        source_agent_id: Option<String>,
        parent_task_id: Option<String>,
        host_context: HostTaskContext,
    ) -> Option<TaskReport> {
        let port = self.state.task_control.read().await.clone()?;
        port.spawn_and_wait(
            agent_id,
            prompt,
            source_agent_id,
            parent_task_id,
            host_context,
            None,
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
    ) -> String {
        let port = match self.state.task_control.read().await.clone() {
            Some(port) => port,
            None => return String::new(),
        };
        port.spawn_detached(
            agent_id,
            prompt,
            source_agent_id,
            parent_task_id,
            host_context,
            None,
        )
        .await
        .unwrap_or_default()
    }

    pub async fn poll_task(&self, task_id: &str) -> Option<TaskReport> {
        let port = self.state.task_control.read().await.clone()?;
        port.poll_task(task_id).await
    }

    pub async fn steer_task(&self, task_id: &str, message: &str) -> bool {
        let port = match self.state.task_control.read().await.clone() {
            Some(port) => port,
            None => return false,
        };
        port.steer_task(task_id, message, None, None).await
    }

    // ---- Result recording (called by task drivers and host integration) ----

    pub async fn record_task_result(&self, task_id: &str, report: TaskReport) {
        self.state
            .registry
            .record_task_result(task_id, report)
            .await;
    }
}
