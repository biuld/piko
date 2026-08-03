use std::collections::HashMap;
use std::sync::Arc;

use piko_llmd::gateway::LlmGateway;
use piko_orchd::AgentRuntime;
use piko_orchd::tools::{
    UserInteractionCallbacks, UserInteractionProvider, UserInteractionRequest,
};
use piko_protocol::tools::{ToolSet, ToolSetToolRef};

use crate::adapters::turns::approval::ApprovalStore;
use crate::api::UserInteractionResponse;
use crate::domain::config::{
    ApprovalSettings, FeaturesSettings, GuardianSettings, McpServerConfig, SafetySettings,
    SandboxSettings, TranscriptSettings,
};
use crate::domain::guardian::{GuardianConfig, GuardianReviewCallback, GuardianState};
use crate::domain::permissions::{PermissionConfig, ResolvedPermissions};
use crate::domain::safety::SafetyConfig;

mod agent_commit;
mod agent_input;
mod approval_gateway;
mod attach;
mod commit;
mod observation_router;
mod prompt_assembly;
mod run;
mod turn_runner;

#[cfg(test)]
mod tests;

use commit::{ExecutionCommitRouter, RealtimeDeltaRouter};

type AgentRunKey = (String, String);
type AgentHubMap = HashMap<AgentRunKey, Arc<piko_orchd::events::SessionOutputHub>>;

#[derive(Clone)]
pub struct OrchAgentRunRunner {
    agent_runtime: Arc<AgentRuntime>,
    active_agent_runs: Arc<std::sync::Mutex<HashMap<AgentRunKey, ActiveAgentRunRuntime>>>,
    agent_hubs: Arc<std::sync::Mutex<AgentHubMap>>,
    commit_routers: Arc<std::sync::Mutex<HashMap<String, Arc<ExecutionCommitRouter>>>>,
    realtime_routers: Arc<std::sync::Mutex<HashMap<String, Arc<RealtimeDeltaRouter>>>>,
    pending_approvals: Arc<std::sync::Mutex<HashMap<String, PendingApprovalEntry>>>,
    pending_interactions: Arc<std::sync::Mutex<HashMap<String, PendingInteractionEntry>>>,
    approval_stores: Arc<std::sync::Mutex<HashMap<String, Arc<ApprovalStore>>>>,
    approval_timeout: std::time::Duration,
    guardian_config: Option<GuardianConfig>,
    guardian_review: Arc<std::sync::RwLock<Option<GuardianReviewCallback>>>,
    guardian_states: Arc<std::sync::Mutex<HashMap<String, GuardianState>>>,
    safety_config: SafetyConfig,
    permission_config: PermissionConfig,
    /// F-19: role → command policy for the approval gateway. Absent roles
    /// use `permission_config` (the session profile).
    role_permission_configs: HashMap<String, PermissionConfig>,
    session_contexts: Arc<std::sync::Mutex<HashMap<String, String>>>,
    session_attach_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    observation_router: Arc<observation_router::SessionObservationRouter>,
    prompt_gate: Arc<tokio::sync::Mutex<()>>,
    context_tools: Arc<piko_orchd::tools::ContextToolsProvider>,
}

struct ActiveAgentRunRuntime {
    run_id: String,
    agent_instance_id: String,
    observation: Arc<piko_orchd::events::SessionOutputHub>,
}

struct PendingApprovalEntry {
    session_id: Option<String>,
    snapshot: crate::api::ApprovalSnapshot,
    tx: piko_comms::ReplySender<piko_comms::contracts::ApprovalReply, crate::api::ApprovalDecision>,
}

struct PendingInteractionEntry {
    session_id: Option<String>,
    snapshot: crate::api::UserInteractionSnapshot,
    tx: piko_comms::ReplySender<piko_comms::contracts::InteractionReply, UserInteractionResponse>,
}

impl OrchAgentRunRunner {
    pub async fn new(
        model_executor: Arc<dyn LlmGateway>,
        provider: &str,
        api_key: &str,
        model_id: &str,
    ) -> Self {
        Self::new_with_mcp(
            model_executor,
            provider,
            api_key,
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
            crate::telemetry::handle(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new_with_mcp(
        model_executor: Arc<dyn LlmGateway>,
        provider: &str,
        api_key: &str,
        model_id: &str,
        thinking_level: Option<piko_protocol::model::ThinkingLevel>,
        thinking_level_map: piko_protocol::model::ThinkingLevelMap,
        context_window: u64,
        max_output_tokens: u64,
        mcp_configs: &[McpServerConfig],
        sandbox_settings: Option<&SandboxSettings>,
        approval_settings: Option<&ApprovalSettings>,
        guardian_settings: Option<&GuardianSettings>,
        safety_settings: Option<&SafetySettings>,
        permissions_settings: Option<&crate::domain::config::PermissionsSettings>,
        features_settings: Option<&FeaturesSettings>,
        transcript: Option<&TranscriptSettings>,
        runtime_telemetry: Arc<dyn piko_orchd_api::telemetry::RuntimeTelemetry>,
    ) -> Self {
        use piko_protocol::config::{ModelRef, OrchdConfig, ProviderConfig, SandboxConfig};
        use piko_protocol::model::ModelRunSettings;

        let mut providers = std::collections::HashMap::new();
        providers.insert(
            provider.to_string(),
            ProviderConfig {
                kind: provider.to_string(),
                api_key: api_key.to_string(),
                base_url: None,
                headers: None,
                streaming_fallback: None,
            },
        );

        let default_settings = ModelRunSettings {
            thinking_level,
            allow_tool_calls: true,
            ..Default::default()
        };

        let mut sandbox = sandbox_settings
            .map(|s| SandboxConfig {
                enabled: s.enabled.unwrap_or(false),
                policy_path: s.policy_path.clone(),
                shell_path: s.shell_path.clone(),
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

        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let agents = crate::adapters::prompts::agent_loader::load_agents(&cwd);

        let config = OrchdConfig {
            providers,
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

        if mcp_enabled {
            let registered =
                crate::infra::mcp::initialize_mcp_tools(mcp_configs, agent_runtime.as_ref()).await;
            if !registered.is_empty() {
                tracing::info!("MCP tools registered: {:?}", registered);
            }
        } else if !mcp_configs.is_empty() {
            tracing::info!("Skipping MCP server connections: feature 'mcp' is disabled");
        }

        Self {
            agent_runtime,
            active_agent_runs: Arc::new(std::sync::Mutex::new(HashMap::new())),
            agent_hubs: Arc::new(std::sync::Mutex::new(HashMap::new())),
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

    fn register_session_context(&self, session_id: String, cwd: String) {
        self.session_contexts
            .lock()
            .unwrap()
            .insert(session_id, cwd);
    }

    fn session_attach_lock(&self, session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.session_attach_locks.lock().unwrap();
        Arc::clone(
            locks
                .entry(session_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }

    fn session_cwd(&self, session_id: &str) -> Option<String> {
        self.session_contexts
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
    }

    fn release_session_context_if_idle(&self, session_id: &str) {
        let active = self
            .active_agent_runs
            .lock()
            .unwrap()
            .keys()
            .any(|(active_session_id, _)| active_session_id == session_id);
        if !active {
            self.session_contexts.lock().unwrap().remove(session_id);
            self.agent_hubs
                .lock()
                .unwrap()
                .retain(|(hub_session_id, _), _| hub_session_id != session_id);
        }
    }

    fn get_approval_store(&self, cwd: &str) -> Arc<ApprovalStore> {
        let mut stores = self.approval_stores.lock().unwrap();
        stores
            .entry(cwd.to_string())
            .or_insert_with(|| Arc::new(ApprovalStore::new(cwd)))
            .clone()
    }

    async fn request_user_interaction(
        &self,
        request: UserInteractionRequest,
    ) -> UserInteractionResponse {
        let _prompt_turn = self.prompt_gate.lock().await;
        if !self
            .observation_router
            .has_route(&request.session_id, &request.agent_instance_id)
        {
            return UserInteractionResponse::Cancel {
                reason: Some("No TUI event channel available".into()),
            };
        }
        let interaction_id = format!(
            "interaction_{}_{}",
            request.tool_call_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let (tx, rx) = piko_comms::reply::<piko_comms::contracts::InteractionReply, _>();
        let session_id = request.session_id.clone();
        let snapshot = crate::api::UserInteractionSnapshot {
            interaction_id: interaction_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            agent_id: request.agent_id.clone(),
            tool_call_id: request.tool_call_id.clone(),
            status: crate::api::UserInteractionStatus::Pending,
            title: request.title.clone(),
            questions: request.questions.clone(),
            require_confirm: request.require_confirm,
            auto_resolution_ms: request.auto_resolution_ms,
        };
        {
            let mut pending = self.pending_interactions.lock().unwrap();
            pending.insert(
                interaction_id.clone(),
                PendingInteractionEntry {
                    session_id: Some(session_id.clone()),
                    snapshot: snapshot.clone(),
                    tx,
                },
            );
        }
        self.observation_router
            .publish(
                &session_id,
                &request.agent_instance_id,
                &request.agent_id,
                piko_protocol::agent_runtime::SessionEvent::InteractionRequested {
                    interaction: snapshot,
                },
            )
            .await;
        let response = match rx.await {
            Ok(response) => response,
            Err(_) => UserInteractionResponse::Cancel {
                reason: Some("Interaction channel closed".into()),
            },
        };
        {
            let mut pending = self.pending_interactions.lock().unwrap();
            pending.remove(&interaction_id);
        }
        response
    }

    async fn register_user_interaction_tools_on_execution(
        &self,
        gateway_runner: &OrchAgentRunRunner,
    ) {
        let user_provider = UserInteractionProvider::new();
        let runner = gateway_runner.clone();
        user_provider
            .set_callbacks(UserInteractionCallbacks {
                request_user_input: Some(Arc::new(move |request| {
                    let runner = runner.clone();
                    Box::pin(async move { runner.request_user_interaction(request).await })
                })),
                request_approval: None,
            })
            .await;
        self.agent_runtime
            .register_tool_provider(Box::new(user_provider))
            .await;
        self.agent_runtime
            .register_tool_set(ToolSet {
                id: "user_interaction".into(),
                name: "User Interaction Tools".into(),
                description: Some("Tools that ask the user for input through hostd/TUI".into()),
                metadata: None,
                policy: None,
                tools: vec![ToolSetToolRef::ProviderNamespace {
                    provider_id: "user_interaction".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                }],
            })
            .await;
    }
}
