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
use crate::domain::config::{McpServerConfig, SandboxSettings};

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
    session_contexts: Arc<std::sync::Mutex<HashMap<String, String>>>,
    session_attach_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    observation_router: Arc<observation_router::SessionObservationRouter>,
    prompt_gate: Arc<tokio::sync::Mutex<()>>,
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
            },
        );

        let default_settings = ModelRunSettings {
            thinking_level,
            allow_tool_calls: true,
            ..Default::default()
        };

        let sandbox = sandbox_settings
            .map(|s| SandboxConfig {
                enabled: s.enabled.unwrap_or(false),
                policy_path: s.policy_path.clone(),
                shell_path: s.shell_path.clone(),
            })
            .unwrap_or_default();

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
        };
        let agent_runtime = AgentRuntime::bootstrap(model_executor, config).await;

        let registered =
            crate::infra::mcp::initialize_mcp_tools(mcp_configs, agent_runtime.as_ref()).await;
        if !registered.is_empty() {
            tracing::info!("MCP tools registered: {:?}", registered);
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
            session_contexts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            session_attach_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            observation_router: Arc::new(observation_router::SessionObservationRouter::default()),
            prompt_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
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
