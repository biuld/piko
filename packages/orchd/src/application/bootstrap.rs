// ---- Bootstrap — from_config, init, and model config ----

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::adapters::tools::task_control_provider::TaskControlProvider;
use crate::adapters::tools::todo_provider::TodoProvider;
use crate::domain::model::step::ModelConfig;
use crate::domain::tools::definition::ToolSet;
use crate::ports::model_gateway::LlmGateway;
use piko_protocol::config::{OrchdConfig, SandboxConfig};
use piko_protocol::messages::Model;
use piko_protocol::model::ModelProviderConfig;
use piko_protocol::runtime::OrchModelConfig;

use super::supervisor::Supervisor;
use super::utils::load_sandbox_policy;

impl Supervisor {
    /// Create a fully initialized Supervisor from an OrchdConfig.
    pub async fn from_config(
        model_executor: Arc<dyn LlmGateway>,
        config: OrchdConfig,
    ) -> Arc<Self> {
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

        let mc = model_config.map(|c| ModelConfig {
            model: crate::domain::model::step::ModelSpec {
                id: c.model.id.clone(),
                name: c.model.name.clone(),
                provider: c.model.provider.clone(),
            },
            provider: c.provider,
            settings: c.settings,
            thinking_level_map: c.thinking_level_map,
        });

        let tool_registry = Arc::new(ToolRegistryImpl::new());
        let model_config = Arc::new(RwLock::new(mc));

        let sup = Self::new(
            Arc::clone(&model_executor),
            Arc::clone(&tool_registry),
            Arc::clone(&model_config),
        );

        sup.init_providers(&config.sandbox).await;

        for spec in config.agents.values() {
            sup.register_agent(spec.clone()).await;
        }

        Arc::new(sup)
    }

    /// Register built-in tool providers and sandbox policy.
    async fn init_providers(&self, sandbox: &SandboxConfig) {
        let orch_provider = TaskControlProvider::new(self.to_spawner());
        self.state.tool_registry
            .register_provider(Box::new(orch_provider))
            .await;

        let todo_provider = TodoProvider::new();
        self.state.tool_registry
            .register_provider(Box::new(todo_provider))
            .await;

        let policy = load_sandbox_policy(sandbox);
        let workspace_provider = if let Some(ref shell) = sandbox.shell_path {
            crate::adapters::tools::workspace_provider::WorkspaceToolProvider::with_shell(
                policy, shell.as_str(),
            )
        } else {
            crate::adapters::tools::workspace_provider::WorkspaceToolProvider::new(policy)
        };
        self.state.tool_registry
            .register_provider(Box::new(workspace_provider))
            .await;

        // Register default tool sets
        let builtin_toolset = ToolSet {
            id: "builtin".into(),
            name: "Built-in Tools".into(),
            description: Some("Built-in orchestrator tools".into()),
            metadata: None,
            policy: None,
            tools: vec![
                crate::domain::tools::definition::ToolSetToolRef::ProviderNamespace {
                    provider_id: "orch".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                },
                crate::domain::tools::definition::ToolSetToolRef::ProviderNamespace {
                    provider_id: "todo".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                },
            ],
        };
        self.state.tool_registry.register_tool_set(builtin_toolset).await;

        let workspace_toolset = ToolSet {
            id: "workspace".into(),
            name: "Workspace Tools".into(),
            description: Some("Local workspace tools".into()),
            metadata: None,
            policy: None,
            tools: vec![
                crate::domain::tools::definition::ToolSetToolRef::ProviderNamespace {
                    provider_id: "workspace".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                },
            ],
        };
        self.state.tool_registry.register_tool_set(workspace_toolset).await;
    }

    // ---- Model config ----

    pub async fn set_model_config(&self, config: OrchModelConfig) {
        let model_config = ModelConfig {
            model: crate::domain::model::step::ModelSpec {
                id: config.model.id.clone(),
                name: config.model.name.clone(),
                provider: config.model.provider.clone(),
            },
            provider: config.provider,
            settings: config.settings,
            thinking_level_map: config.thinking_level_map,
        };
        *self.state.model_config.write().await = Some(model_config);
    }
}
