//! Single-agent tool + model bootstrap for `AgentExecutionRuntime`.
//!
//! Intentionally omits TaskControl / multi-agent providers.

use std::sync::Arc;

use piko_protocol::config::OrchdConfig;
use piko_protocol::messages::Model;
use piko_protocol::model::ModelProviderConfig;
use piko_protocol::runtime::OrchModelConfig;
use piko_protocol::tools::{ToolSet, ToolSetToolRef};

use crate::adapters::tools::todo_provider::TodoProvider;
use crate::adapters::tools::workspace_provider::WorkspaceToolProvider;
use crate::domain::model::step::ModelConfig;
use crate::ports::model_gateway::LlmGateway;
use crate::runtime::utils::load_sandbox_policy;

use super::AgentExecutionRuntime;

impl AgentExecutionRuntime {
    /// Build an Execution runtime with workspace/todo tools and configured agents.
    pub async fn bootstrap(model_executor: Arc<dyn LlmGateway>, config: OrchdConfig) -> Arc<Self> {
        let runtime = Arc::new(Self::new(model_executor));
        runtime.install_config(config).await;
        runtime
    }

    async fn install_config(&self, config: OrchdConfig) {
        let model_config = {
            let model = Model {
                id: config.default_model.model_id.clone(),
                name: config.default_model.model_id.clone(),
                provider: config.default_model.provider.clone(),
                base_url: None,
            };
            let provider = config
                .providers
                .get(&config.default_model.provider)
                .map(|p| ModelProviderConfig {
                    api_key: Some(p.api_key.clone()),
                    base_url: p.base_url.clone(),
                    headers: Some(p.headers.clone().unwrap_or_default()),
                    reasoning: None,
                    session_id: None,
                    extra: None,
                })
                .unwrap_or_default();

            Some(OrchModelConfig {
                model,
                provider,
                settings: config.default_settings.clone(),
                thinking_level_map: config.thinking_level_map.clone(),
            })
        };

        if let Some(c) = model_config {
            self.services
                .set_model_config(ModelConfig {
                    model: crate::domain::model::step::ModelSpec {
                        id: c.model.id.clone(),
                        name: c.model.name.clone(),
                        provider: c.model.provider.clone(),
                    },
                    provider: c.provider,
                    settings: c.settings,
                    thinking_level_map: c.thinking_level_map,
                })
                .await;
        }

        self.register_single_agent_tools(&config.sandbox).await;

        for spec in config.agents.values() {
            self.register_agent(spec.clone()).await;
        }
    }

    async fn register_single_agent_tools(&self, sandbox: &piko_protocol::config::SandboxConfig) {
        self.register_tool_provider(Box::new(TodoProvider::new()))
            .await;

        let policy = load_sandbox_policy(sandbox);
        let workspace_provider = if let Some(ref shell) = sandbox.shell_path {
            WorkspaceToolProvider::with_shell(policy, shell.as_str())
        } else {
            WorkspaceToolProvider::new(policy)
        };
        self.register_tool_provider(Box::new(workspace_provider))
            .await;

        // Single-agent packs: todo + workspace. multi_agent is registered by
        // AgentRuntime::bootstrap; user_interaction is registered by hostd.
        self.register_tool_set(ToolSet {
            id: "todo".into(),
            name: "Todo Tools".into(),
            description: Some("Task-list planning tools".into()),
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "todo".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        })
        .await;

        self.register_tool_set(ToolSet {
            id: "workspace".into(),
            name: "Workspace Tools".into(),
            description: Some("Local workspace tools".into()),
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "workspace".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        })
        .await;
    }
}
