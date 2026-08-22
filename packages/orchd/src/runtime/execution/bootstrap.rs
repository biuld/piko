//! Single-agent tool + model bootstrap for `AgentExecutionRuntime`.
//!
//! Intentionally omits TaskControl / multi-agent providers.

use std::sync::Arc;

use piko_protocol::config::OrchdConfig;
use piko_protocol::messages::Model;
use piko_protocol::runtime::OrchModelConfig;
use piko_protocol::tools::{ToolSet, ToolSetToolRef};

use crate::adapters::tools::ToolContribution;
use crate::adapters::tools::todo_provider::TodoProvider;
use crate::adapters::tools::workspace_provider::WorkspaceToolProvider;
use crate::domain::model::step::ModelConfig;
use crate::ports::model_gateway::InferenceGateway;
use crate::runtime::utils::{load_role_sandbox_policies, load_sandbox_policy};
use piko_orchd_api::telemetry::RuntimeTelemetry;

use super::AgentExecutionRuntime;

impl AgentExecutionRuntime {
    /// Build an Execution runtime with workspace/todo tools and configured agents.
    pub async fn bootstrap(
        model_executor: Arc<dyn InferenceGateway>,
        config: OrchdConfig,
    ) -> Arc<Self> {
        Self::bootstrap_with_telemetry(
            model_executor,
            config,
            Arc::new(piko_orchd_api::telemetry::NoopRuntimeTelemetry),
        )
        .await
    }

    /// Like [`bootstrap`], with a hostd-provided telemetry sink for metrics.
    pub async fn bootstrap_with_telemetry(
        model_executor: Arc<dyn InferenceGateway>,
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
            };
            Some(OrchModelConfig {
                model,
                settings: config.default_settings.clone(),
            })
        };

        if let Some(c) = model_config {
            let model_ref = piko_llmd::gateway::ModelRef::new(&c.model.provider, &c.model.id);
            match self.services.model_executor().describe(&model_ref).await {
                Ok(descriptor) => {
                    self.services
                        .tool_registry()
                        .register_upstream_catalog(&descriptor)
                        .await;
                }
                Err(error) => tracing::debug!(
                    provider = %c.model.provider,
                    model = %c.model.id,
                    %error,
                    "model gateway does not expose an upstream tool catalog"
                ),
            }
            self.services
                .set_model_config(ModelConfig {
                    model: crate::domain::model::step::ModelSpec {
                        id: c.model.id.clone(),
                        name: c.model.name.clone(),
                        provider: c.model.provider.clone(),
                    },
                    settings: c.settings,
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
        // One shared TodoProvider: registry clone + seed handle share Arc state.
        let todo = TodoProvider::new();
        self.services.set_todo_provider(todo.clone()).await;
        let todo_set = ToolSet {
            id: "todo".into(),
            name: "Todo Tools".into(),
            description: Some("Task-list planning tools".into()),
            feature: Some(piko_protocol::tools::ToolSetFeature::Family { key: "todo".into() }),
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "todo".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        };
        self.install_tool_contribution(ToolContribution {
            provider: Box::new(todo),
            tool_sets: vec![todo_set],
        })
        .await
        .expect("built-in todo tool contribution is valid");

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
        let workspace_set = ToolSet {
            id: "workspace".into(),
            name: "Workspace Tools".into(),
            description: Some("Local workspace tools".into()),
            feature: Some(piko_protocol::tools::ToolSetFeature::ByTool {
                tool_features: std::collections::HashMap::from([
                    ("read".into(), "workspace".into()),
                    ("edit".into(), "workspace".into()),
                    ("write".into(), "workspace".into()),
                    ("exec_command".into(), "exec".into()),
                    ("write_stdin".into(), "exec".into()),
                    ("environment".into(), "environment".into()),
                ]),
            }),
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "workspace".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        };
        self.install_tool_contribution(ToolContribution {
            provider: Box::new(workspace_provider),
            tool_sets: vec![workspace_set],
        })
        .await
        .expect("built-in workspace tool contribution is valid");
    }
}
