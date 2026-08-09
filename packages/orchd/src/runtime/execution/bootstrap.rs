//! Single-agent tool + model bootstrap for `AgentExecutionRuntime`.
//!
//! Intentionally omits TaskControl / multi-agent providers.

use std::sync::Arc;

use piko_protocol::config::OrchdConfig;
use piko_protocol::messages::Model;
use piko_protocol::runtime::OrchModelConfig;
use piko_protocol::tools::{ToolSet, ToolSetToolRef};

use crate::adapters::tools::todo_provider::TodoProvider;
use crate::adapters::tools::workspace_provider::WorkspaceToolProvider;
use crate::domain::model::step::ModelConfig;
use crate::ports::model_gateway::LlmGateway;
use crate::runtime::utils::{load_role_sandbox_policies, load_sandbox_policy};
use piko_orchd_api::telemetry::RuntimeTelemetry;

use super::AgentExecutionRuntime;

impl AgentExecutionRuntime {
    /// Build an Execution runtime with workspace/todo tools and configured agents.
    pub async fn bootstrap(model_executor: Arc<dyn LlmGateway>, config: OrchdConfig) -> Arc<Self> {
        Self::bootstrap_with_telemetry(
            model_executor,
            config,
            Arc::new(piko_orchd_api::telemetry::NoopRuntimeTelemetry),
        )
        .await
    }

    /// Like [`bootstrap`], with a hostd-provided telemetry sink for metrics.
    pub async fn bootstrap_with_telemetry(
        model_executor: Arc<dyn LlmGateway>,
        config: OrchdConfig,
        telemetry: Arc<dyn RuntimeTelemetry>,
    ) -> Arc<Self> {
        let runtime = Arc::new(Self::with_telemetry(model_executor, telemetry));
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
            Some(OrchModelConfig {
                model,
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
                    settings: c.settings,
                    thinking_level_map: c.thinking_level_map,
                    context_window: config.default_model.context_window,
                    max_output_tokens: config.default_model.max_output_tokens,
                    max_tool_output_tokens: config.transcript_max_tool_output_tokens,
                })
                .await;
        }

        self.register_single_agent_tools(&config.sandbox).await;
        // F-18 managed features: install the resolved feature set once so the
        // registry can gate the catalog and classify direct calls.
        self.services
            .tool_registry()
            .set_features(config.features.clone())
            .await;

        for spec in config.agents.values() {
            self.register_agent(spec.clone()).await;
        }
    }

    async fn register_single_agent_tools(&self, sandbox: &piko_protocol::config::SandboxConfig) {
        self.register_tool_provider(Box::new(TodoProvider::new()))
            .await;

        let policy = load_sandbox_policy(sandbox);
        // F-19: attach per-role sandbox policies so workspace tools select
        // the executing agent's role policy (session policy is the
        // fallback for unmapped roles).
        let role_policies = load_role_sandbox_policies(sandbox);
        let workspace_provider = if let Some(ref shell) = sandbox.shell_path {
            WorkspaceToolProvider::with_shell(policy, shell.as_str(), Arc::clone(&self.processes))
                .with_role_policies(role_policies)
        } else {
            WorkspaceToolProvider::new(policy, Arc::clone(&self.processes))
                .with_role_policies(role_policies)
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
