use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::adapters::tools::ToolContribution;
use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::adapters::tools::todo_provider::TodoProvider;
use crate::domain::model::step::ModelConfig;
use crate::ports::model_gateway::InferenceGateway;
use piko_orchd_api::ToolProvider;
use piko_orchd_api::telemetry::{NoopRuntimeTelemetry, RuntimeTelemetry};
use piko_protocol::TodoList;
use piko_protocol::agents::AgentSpec;
use piko_protocol::tools::ToolSet;

#[derive(Clone)]
pub struct ExecutionServices {
    model_executor: Arc<dyn InferenceGateway>,
    agent_specs: Arc<RwLock<HashMap<String, AgentSpec>>>,
    model_config: Arc<RwLock<Option<ModelConfig>>>,
    tool_registry: Arc<ToolRegistryImpl>,
    telemetry: Arc<dyn RuntimeTelemetry>,
    /// Shared with the registered todo tool provider (same Arc state).
    todo_provider: Arc<RwLock<Option<TodoProvider>>>,
}

impl ExecutionServices {
    pub fn new(model_executor: Arc<dyn InferenceGateway>) -> Self {
        Self::with_telemetry(model_executor, Arc::new(NoopRuntimeTelemetry))
    }

    pub fn with_telemetry(
        model_executor: Arc<dyn InferenceGateway>,
        telemetry: Arc<dyn RuntimeTelemetry>,
    ) -> Self {
        Self {
            model_executor,
            agent_specs: Arc::new(RwLock::new(HashMap::new())),
            model_config: Arc::new(RwLock::new(None)),
            tool_registry: Arc::new(ToolRegistryImpl::new()),
            telemetry,
            todo_provider: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn register_agent(&self, spec: AgentSpec) {
        self.agent_specs.write().await.insert(spec.id.clone(), spec);
    }

    pub async fn agent_spec(&self, agent_id: &str) -> Option<AgentSpec> {
        self.agent_specs.read().await.get(agent_id).cloned()
    }

    /// Snapshot of registered AgentSpecs, sorted by id (F-21 catalog).
    pub async fn list_agent_specs(&self) -> Vec<AgentSpec> {
        let mut specs: Vec<AgentSpec> = self.agent_specs.read().await.values().cloned().collect();
        specs.sort_by(|left, right| left.id.cmp(&right.id));
        specs
    }

    pub fn model_executor(&self) -> Arc<dyn InferenceGateway> {
        Arc::clone(&self.model_executor)
    }

    pub async fn set_model_config(&self, config: ModelConfig) {
        *self.model_config.write().await = Some(config);
    }

    pub async fn model_config(&self) -> Option<ModelConfig> {
        self.model_config.read().await.clone()
    }

    pub fn tool_registry(&self) -> Arc<ToolRegistryImpl> {
        Arc::clone(&self.tool_registry)
    }

    pub fn telemetry(&self) -> Arc<dyn RuntimeTelemetry> {
        Arc::clone(&self.telemetry)
    }

    pub async fn register_tool_provider(&self, provider: Box<dyn ToolProvider>) {
        self.tool_registry.register_provider(provider).await;
    }

    pub async fn register_tool_set(&self, tool_set: ToolSet) {
        self.tool_registry.register_tool_set(tool_set).await;
    }

    pub async fn install_tool_contribution(
        &self,
        contribution: ToolContribution,
    ) -> Result<(), String> {
        self.tool_registry.install_contribution(contribution).await
    }

    /// Keep a clone of the todo provider so host can seed durable lists.
    pub async fn set_todo_provider(&self, provider: TodoProvider) {
        *self.todo_provider.write().await = Some(provider);
    }

    /// Seed runtime todo store from host durable lists (session hydrate).
    pub async fn seed_todo_lists(&self, lists: impl IntoIterator<Item = TodoList>) {
        let guard = self.todo_provider.read().await;
        if let Some(provider) = guard.as_ref() {
            provider.seed_from_lists(lists).await;
        }
    }
}
