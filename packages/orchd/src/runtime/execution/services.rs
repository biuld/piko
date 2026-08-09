use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::model::step::ModelConfig;
use crate::ports::model_gateway::InferenceGateway;
use piko_orchd_api::ToolProvider;
use piko_orchd_api::telemetry::{NoopRuntimeTelemetry, RuntimeTelemetry};
use piko_protocol::agents::AgentSpec;
use piko_protocol::tools::ToolSet;

#[derive(Clone)]
pub struct ExecutionServices {
    model_executor: Arc<dyn InferenceGateway>,
    agent_specs: Arc<RwLock<HashMap<String, AgentSpec>>>,
    model_config: Arc<RwLock<Option<ModelConfig>>>,
    tool_registry: Arc<ToolRegistryImpl>,
    telemetry: Arc<dyn RuntimeTelemetry>,
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
}
