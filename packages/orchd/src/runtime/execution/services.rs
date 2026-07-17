use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::model::step::ModelConfig;
use crate::ports::model_gateway::LlmGateway;
use piko_orchd_api::ToolProvider;
use piko_protocol::agents::AgentSpec;
use piko_protocol::tools::ToolSet;

#[derive(Clone)]
pub struct ExecutionServices {
    model_executor: Arc<dyn LlmGateway>,
    agent_specs: Arc<RwLock<HashMap<String, AgentSpec>>>,
    model_config: Arc<RwLock<Option<ModelConfig>>>,
    tool_registry: Arc<ToolRegistryImpl>,
}

impl ExecutionServices {
    pub fn new(model_executor: Arc<dyn LlmGateway>) -> Self {
        Self {
            model_executor,
            agent_specs: Arc::new(RwLock::new(HashMap::new())),
            model_config: Arc::new(RwLock::new(None)),
            tool_registry: Arc::new(ToolRegistryImpl::new()),
        }
    }

    pub async fn register_agent(&self, spec: AgentSpec) {
        self.agent_specs.write().await.insert(spec.id.clone(), spec);
    }

    pub async fn agent_spec(&self, agent_id: &str) -> Option<AgentSpec> {
        self.agent_specs.read().await.get(agent_id).cloned()
    }

    pub fn model_executor(&self) -> Arc<dyn LlmGateway> {
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

    pub async fn register_tool_provider(&self, provider: Box<dyn ToolProvider>) {
        self.tool_registry.register_provider(provider).await;
    }

    pub async fn register_tool_set(&self, tool_set: ToolSet) {
        self.tool_registry.register_tool_set(tool_set).await;
    }
}
