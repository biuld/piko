use std::sync::Arc;

use orchd_api::{ApprovalGateway, PersistSink, ToolProvider};
use piko_protocol::agents::AgentSpec;
use piko_protocol::config::OrchdConfig;
use piko_protocol::tools::ToolSet;

use crate::application::Supervisor;
use crate::application::service::AgentRuntimeService;
use crate::ports::model_gateway::LlmGateway;

/// Bootstrapped in-process agent runtime.
pub struct Runtime {
    supervisor: Arc<Supervisor>,
}

impl Runtime {
    pub async fn bootstrap(model_executor: Arc<dyn LlmGateway>, config: OrchdConfig) -> Arc<Self> {
        let supervisor = Supervisor::from_config(model_executor, config).await;
        Arc::new(Self { supervisor })
    }

    pub fn agent_runtime(&self) -> AgentRuntimeService {
        AgentRuntimeService::new(Arc::clone(&self.supervisor))
    }

    pub async fn register_agent(&self, spec: AgentSpec) {
        self.supervisor.register_agent(spec).await;
    }

    pub async fn set_persist_sink(&self, sink: Arc<dyn PersistSink>) {
        self.supervisor.set_persist_sink(sink).await;
    }

    pub async fn register_tool_provider(&self, provider: Box<dyn ToolProvider>) {
        self.supervisor
            .tool_registry()
            .register_provider(provider)
            .await;
    }

    pub async fn register_tool_set(&self, tool_set: ToolSet) {
        self.supervisor.register_tool_set(tool_set).await;
    }

    pub async fn set_approval_gateway(&self, gateway: Box<dyn ApprovalGateway>) {
        self.supervisor
            .tool_registry()
            .set_approval_gateway(Some(gateway))
            .await;
    }
}
