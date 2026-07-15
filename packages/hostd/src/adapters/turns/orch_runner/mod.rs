use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use llmd::gateway::LlmGateway;
use orchd::AgentRuntime;
use orchd::tools::{UserInteractionCallbacks, UserInteractionProvider, UserInteractionRequest};
use piko_protocol::tools::{ToolSet, ToolSetToolRef};

use crate::adapters::turns::approval::ApprovalStore;
use crate::api::{ServerMessage, UserInteractionResponse};
use crate::domain::config::{McpServerConfig, SandboxSettings};

mod agent_commit;
mod approval_gateway;
mod commit;
mod completion;
mod prompt_assembly;
mod run;
mod turn_runner;

#[cfg(test)]
mod tests;

use commit::{ExecutionCommitRouter, RealtimeDeltaRouter};

#[derive(Clone)]
pub struct OrchTurnRunner {
    agent_runtime: Arc<AgentRuntime>,
    /// session_id -> active root Turn. Execution identity stays inside AgentRuntime.
    active_turns: Arc<std::sync::Mutex<HashMap<String, ActiveTurnRuntime>>>,
    commit_routers: Arc<std::sync::Mutex<HashMap<String, Arc<ExecutionCommitRouter>>>>,
    realtime_routers: Arc<std::sync::Mutex<HashMap<String, Arc<RealtimeDeltaRouter>>>>,
    pending_approvals: Arc<std::sync::Mutex<HashMap<String, PendingApprovalEntry>>>,
    pending_interactions: Arc<std::sync::Mutex<HashMap<String, PendingInteractionEntry>>>,
    approval_stores: Arc<std::sync::Mutex<HashMap<String, Arc<ApprovalStore>>>>,
    session_contexts: Arc<std::sync::Mutex<HashMap<String, String>>>,
    agent_event_tx: Arc<std::sync::Mutex<Option<UnboundedSender<ServerMessage>>>>,
    ui_event_tx: Option<UnboundedSender<ServerMessage>>,
    prompt_gate: Arc<tokio::sync::Mutex<()>>,
}

struct ActiveTurnRuntime {
    turn_id: String,
    observation: Arc<orchd::testing::SessionOutputHub>,
    durable_commit: Arc<dyn orchd_api::ExecutionCommitPort>,
}

struct PendingApprovalEntry {
    session_id: Option<String>,
    snapshot: crate::api::ApprovalSnapshot,
    tx: oneshot::Sender<crate::api::ApprovalDecision>,
}

struct PendingInteractionEntry {
    session_id: Option<String>,
    snapshot: crate::api::UserInteractionSnapshot,
    tx: oneshot::Sender<UserInteractionResponse>,
}

fn remove_active_turn_if_matches(
    active: &mut HashMap<String, ActiveTurnRuntime>,
    session_id: &str,
    turn_id: &str,
) -> Option<ActiveTurnRuntime> {
    if active
        .get(session_id)
        .is_some_and(|active_turn| active_turn.turn_id == turn_id)
    {
        active.remove(session_id)
    } else {
        None
    }
}

impl OrchTurnRunner {
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
            active_turns: Arc::new(std::sync::Mutex::new(HashMap::new())),
            commit_routers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            realtime_routers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_approvals: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_interactions: Arc::new(std::sync::Mutex::new(HashMap::new())),
            approval_stores: Arc::new(std::sync::Mutex::new(HashMap::new())),
            session_contexts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            agent_event_tx: Arc::new(std::sync::Mutex::new(None)),
            ui_event_tx: None,
            prompt_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn with_ui_event_tx(&self, ui_event_tx: UnboundedSender<ServerMessage>) -> Self {
        *self.agent_event_tx.lock().unwrap() = Some(ui_event_tx.clone());
        Self {
            agent_runtime: Arc::clone(&self.agent_runtime),
            active_turns: Arc::clone(&self.active_turns),
            commit_routers: Arc::clone(&self.commit_routers),
            realtime_routers: Arc::clone(&self.realtime_routers),
            pending_approvals: Arc::clone(&self.pending_approvals),
            pending_interactions: Arc::clone(&self.pending_interactions),
            approval_stores: Arc::clone(&self.approval_stores),
            session_contexts: Arc::clone(&self.session_contexts),
            agent_event_tx: Arc::clone(&self.agent_event_tx),
            ui_event_tx: Some(ui_event_tx),
            prompt_gate: Arc::clone(&self.prompt_gate),
        }
    }

    fn register_session_context(&self, session_id: String, cwd: String) {
        self.session_contexts
            .lock()
            .unwrap()
            .insert(session_id, cwd);
    }

    fn session_cwd(&self, session_id: &str) -> Option<String> {
        self.session_contexts
            .lock()
            .unwrap()
            .get(session_id)
            .cloned()
    }

    fn get_approval_store(&self, cwd: &str) -> Arc<ApprovalStore> {
        let mut stores = self.approval_stores.lock().unwrap();
        stores
            .entry(cwd.to_string())
            .or_insert_with(|| Arc::new(ApprovalStore::new(cwd)))
            .clone()
    }

    fn emit_ui_event(&self, event: ServerMessage) {
        if let Some(tx) = self.ui_event_tx.as_ref()
            && tx.send(event).is_err()
        {
            tracing::error!("turn ui event channel closed");
        }
    }

    async fn request_user_interaction(
        &self,
        request: UserInteractionRequest,
    ) -> UserInteractionResponse {
        let _prompt_turn = self.prompt_gate.lock().await;
        let Some(_) = self.ui_event_tx.as_ref() else {
            return UserInteractionResponse::Cancel {
                reason: Some("No TUI event channel available".into()),
            };
        };
        let interaction_id = format!(
            "interaction_{}_{}",
            request.tool_call_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let (tx, rx) = oneshot::channel();
        let session_id = request.session_id.clone();
        {
            let mut pending = self.pending_interactions.lock().unwrap();
            pending.insert(
                interaction_id.clone(),
                PendingInteractionEntry {
                    session_id: Some(session_id.clone()),
                    snapshot: crate::api::UserInteractionSnapshot {
                        interaction_id: interaction_id.clone(),
                        agent_instance_id: request.agent_instance_id.clone(),
                        agent_id: request.agent_id.clone(),
                        tool_call_id: request.tool_call_id.clone(),
                        status: crate::api::UserInteractionStatus::Pending,
                        title: request.title.clone(),
                        questions: request.questions.clone(),
                        require_confirm: request.require_confirm,
                        auto_resolution_ms: request.auto_resolution_ms,
                    },
                    tx,
                },
            );
        }
        self.emit_ui_event(ServerMessage::Interaction(
            piko_protocol::InteractionEvent::Requested {
                session_id,
                agent_instance_id: request.agent_instance_id.clone(),
                agent_id: request.agent_id.clone(),
                interaction_id: interaction_id.clone(),
                tool_call_id: request.tool_call_id,
                title: request.title,
                questions: request.questions,
                require_confirm: request.require_confirm,
                auto_resolution_ms: request.auto_resolution_ms,
            },
        ));
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

    async fn register_user_interaction_tools_on_execution(&self, gateway_runner: &OrchTurnRunner) {
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
