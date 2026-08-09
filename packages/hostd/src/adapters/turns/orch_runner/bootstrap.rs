use piko_llmd::gateway::LlmGateway;

use crate::domain::config::{
    ApprovalSettings, ExecutionSettings, FeaturesSettings, GuardianSettings, McpServerConfig,
    SafetySettings, TranscriptSettings,
};
use crate::domain::permissions::ResolvedPermissions;

use super::*;

impl OrchAgentRunRunner {
    pub async fn new(model_executor: Arc<dyn LlmGateway>, provider: &str, model_id: &str) -> Self {
        Self::new_with_mcp(
            model_executor,
            provider,
            model_id,
            None,
            None,
            128_000,
            4_096,
            &[],
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            crate::telemetry::handle(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new_with_mcp(
        model_executor: Arc<dyn LlmGateway>,
        provider: &str,
        model_id: &str,
        thinking_level: Option<piko_protocol::model::ThinkingLevel>,
        thinking_level_map: piko_protocol::model::ThinkingLevelMap,
        context_window: u64,
        max_output_tokens: u64,
        mcp_configs: &[McpServerConfig],
        mcp_settings: Option<&crate::domain::config::McpSettings>,
        execution_settings: Option<&ExecutionSettings>,
        approval_settings: Option<&ApprovalSettings>,
        guardian_settings: Option<&GuardianSettings>,
        safety_settings: Option<&SafetySettings>,
        permissions_settings: Option<&crate::domain::config::PermissionsSettings>,
        features_settings: Option<&FeaturesSettings>,
        transcript: Option<&TranscriptSettings>,
        runtime_telemetry: Arc<dyn piko_orchd_api::telemetry::RuntimeTelemetry>,
    ) -> Self {
        use piko_protocol::config::{ModelRef, OrchdConfig, SandboxConfig};
        use piko_protocol::model::ModelRunSettings;

        let default_settings = ModelRunSettings {
            thinking_level,
            allow_tool_calls: true,
            ..Default::default()
        };

        let mut sandbox = execution_settings
            .map(|s| SandboxConfig {
                shell_path: s.shell.clone(),
                policy_profile: None,
                role_policies: std::collections::HashMap::new(),
            })
            .unwrap_or_default();

        // F-17 permission profiles: materialize the resolved profile's
        // file/network policy into the sandbox config (the orchestrator
        // inherits the execution whitelist). Command rules go to the
        // approval gateway via `permission_config`.
        let resolved_permissions: ResolvedPermissions =
            crate::domain::permissions::resolve_permissions(permissions_settings);
        let permission_config = resolved_permissions.config.clone();
        let role_permission_configs = resolved_permissions.role_configs.clone();
        if resolved_permissions.materialize {
            let profile = &resolved_permissions.profile;
            sandbox.policy_profile = Some(piko_protocol::config::PermissionPolicy {
                read_roots: profile.read_roots.clone(),
                write_roots: profile.write_roots.clone(),
                scratch_roots: profile.scratch_roots.clone(),
                deny_paths: profile.deny_paths.clone(),
                allow_network: profile.allow_network,
            });
        }
        // F-19: materialize per-role file/network policies for the sandbox.
        // Roles without an entry keep the session policy in orchd.
        sandbox.role_policies = resolved_permissions
            .role_policies
            .iter()
            .map(|(role, policy)| {
                (
                    role.clone(),
                    piko_protocol::config::PermissionPolicy {
                        read_roots: policy.read_roots.clone(),
                        write_roots: policy.write_roots.clone(),
                        scratch_roots: policy.scratch_roots.clone(),
                        deny_paths: policy.deny_paths.clone(),
                        allow_network: policy.allow_network,
                    },
                )
            })
            .collect();

        // F-18 managed features: resolve once at session bootstrap. The full
        // resolved map goes to orchd for catalog gating; hostd skips MCP
        // server connections when the `mcp` feature is disabled.
        let resolved_features = crate::domain::features::resolve_features(features_settings);
        for warning in &resolved_features.warnings {
            tracing::warn!("[features] {warning}");
        }
        let mcp_enabled = resolved_features
            .enabled
            .get("mcp")
            .copied()
            .unwrap_or(true);
        // F-13 prewarm: eager connect at session start under a bounded
        // per-server timeout (default 10 s). A slow/broken server is skipped
        // with a warning; the others register.
        let mcp_connect_timeout = std::time::Duration::from_millis(
            mcp_settings
                .and_then(|settings| settings.connect_timeout_ms)
                .unwrap_or(10_000),
        );

        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let agents = crate::adapters::prompts::agent_loader::load_agents(&cwd);

        let config = OrchdConfig {
            agents,
            default_model: ModelRef {
                provider: provider.to_string(),
                model_id: model_id.to_string(),
                context_window,
                max_output_tokens,
            },
            default_settings,
            runtime: Default::default(),
            thinking_level_map,
            sandbox,
            transcript_max_tool_output_tokens: transcript
                .and_then(|settings| settings.max_tool_output_tokens)
                .unwrap_or(24_000),
            features: Some(resolved_features.enabled),
        };
        let agent_runtime =
            AgentRuntime::bootstrap_with_telemetry(model_executor, config, runtime_telemetry).await;
        let context_tools = agent_runtime.context_tools();

        let mcp_server_statuses = if mcp_enabled {
            let statuses = crate::infra::mcp::initialize_mcp_tools(
                mcp_configs,
                mcp_connect_timeout,
                agent_runtime.as_ref(),
            )
            .await;
            let connected: Vec<_> = statuses
                .iter()
                .filter(|status| status.connected)
                .map(|status| status.name.clone())
                .collect();
            if !connected.is_empty() {
                tracing::info!("MCP tools registered: {:?}", connected);
            }
            statuses
        } else {
            if !mcp_configs.is_empty() {
                tracing::info!("Skipping MCP server connections: feature 'mcp' is disabled");
            }
            // Report configured-but-disabled servers honestly on the
            // `mcp.status` surface instead of hiding them.
            mcp_configs
                .iter()
                .map(|config| piko_protocol::command::McpServerInfo {
                    name: config.name.clone(),
                    connected: false,
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                    error: Some("feature 'mcp' is disabled".into()),
                })
                .collect()
        };

        Self {
            agent_runtime,
            active_agent_runs: Arc::new(std::sync::Mutex::new(HashMap::new())),
            agent_hubs: Arc::new(std::sync::Mutex::new(HashMap::new())),
            prompt_debug_snapshots: Arc::new(std::sync::Mutex::new(HashMap::new())),
            commit_routers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            realtime_routers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_approvals: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_interactions: Arc::new(std::sync::Mutex::new(HashMap::new())),
            approval_stores: Arc::new(std::sync::Mutex::new(HashMap::new())),
            approval_timeout: std::time::Duration::from_secs(
                approval_settings
                    .and_then(|settings| settings.timeout_secs)
                    .unwrap_or(120)
                    .max(1),
            ),
            guardian_config: GuardianConfig::from_settings(guardian_settings),
            guardian_review: Arc::new(std::sync::RwLock::new(None)),
            guardian_states: Arc::new(std::sync::Mutex::new(HashMap::new())),
            safety_config: SafetyConfig::from_settings(safety_settings),
            permission_config,
            mcp_approval_templates: mcp_settings
                .map(|settings| settings.approval_templates.clone())
                .unwrap_or_default(),
            mcp_server_names: mcp_configs
                .iter()
                .map(|config| config.name.clone())
                .collect(),
            mcp_server_statuses,
            role_permission_configs,
            session_contexts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            session_attach_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            observation_router: Arc::new(observation_router::SessionObservationRouter::default()),
            prompt_gate: Arc::new(tokio::sync::Mutex::new(())),
            context_tools,
        }
    }

    /// Wire the `new_context_window` tool callback (F-05). Hostd invokes the
    /// token-budget compact so the rewrite stays host-owned.
    pub fn set_context_window_callback(
        &self,
        callback: piko_orchd::tools::NewContextWindowCallback,
    ) {
        let provider = Arc::clone(&self.context_tools);
        tokio::spawn(async move {
            provider
                .set_callbacks(piko_orchd::tools::ContextToolsCallbacks {
                    new_context_window: Some(callback),
                })
                .await;
        });
    }

    /// Wire the guardian review callback (F-11). Hostd runs the bounded
    /// review over the durable session tree; the gateway only consumes the
    /// decision.
    pub fn set_guardian_review_callback(&self, callback: GuardianReviewCallback) {
        *self.guardian_review.write().unwrap() = Some(callback);
    }
}
