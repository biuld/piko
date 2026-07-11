use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use llmd::gateway::LlmGateway;
use orchd::Runtime;
use orchd::tools::{UserInteractionCallbacks, UserInteractionProvider, UserInteractionRequest};
use orchd_api::{
    AgentRuntime, ApprovalGateway, PersistSink, SessionSubscription, ToolApprovalDecision,
    ToolApprovalRequest, build_user_input,
};
use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::InputSource;
use piko_protocol::agents::AgentSpec;
use piko_protocol::tools::{ToolSet, ToolSetToolRef};

use crate::api::{ProtocolError, ServerMessage, UserInteractionResponse};
use crate::domain::config::{McpServerConfig, SandboxSettings};
use crate::domain::turns::approval::{ApprovalScope, ApprovalStore};
use crate::domain::turns::runner::{TurnRunInput, TurnRunner};

#[derive(Clone)]
pub struct OrchTurnRunner {
    runtime: Arc<Runtime>,
    pending_approvals:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<crate::api::ApprovalDecision>>>>,
    pending_interactions:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<UserInteractionResponse>>>>,
    approval_stores: Arc<std::sync::Mutex<HashMap<String, Arc<ApprovalStore>>>>,
    task_contexts: Arc<std::sync::Mutex<HashMap<String, (String, String)>>>, // task_id -> (session_id, cwd)
    ui_event_tx: Option<UnboundedSender<ServerMessage>>,
    prompt_gate: Arc<tokio::sync::Mutex<()>>,
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
        let agents = crate::domain::agents::loader::load_agents(&cwd);

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
        let runtime = Runtime::bootstrap(model_executor, config).await;

        let registered =
            crate::infra::mcp::initialize_mcp_tools(mcp_configs, runtime.as_ref()).await;
        if !registered.is_empty() {
            tracing::info!("MCP tools registered: {:?}", registered);
        }

        Self {
            runtime,
            pending_approvals: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_interactions: Arc::new(std::sync::Mutex::new(HashMap::new())),
            approval_stores: Arc::new(std::sync::Mutex::new(HashMap::new())),
            task_contexts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            ui_event_tx: None,
            prompt_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn with_ui_event_tx(&self, ui_event_tx: UnboundedSender<ServerMessage>) -> Self {
        Self {
            runtime: Arc::clone(&self.runtime),
            pending_approvals: Arc::clone(&self.pending_approvals),
            pending_interactions: Arc::clone(&self.pending_interactions),
            approval_stores: Arc::clone(&self.approval_stores),
            task_contexts: Arc::clone(&self.task_contexts),
            ui_event_tx: Some(ui_event_tx),
            prompt_gate: Arc::clone(&self.prompt_gate),
        }
    }

    fn register_task_context(&self, task_id: String, session_id: String, cwd: String) {
        let mut contexts = self.task_contexts.lock().unwrap();
        contexts.insert(task_id, (session_id, cwd));
    }

    fn get_task_context(&self, task_id: &str) -> Option<(String, String)> {
        let contexts = self.task_contexts.lock().unwrap();
        contexts.get(task_id).cloned()
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
        {
            let mut pending = self.pending_interactions.lock().unwrap();
            pending.insert(interaction_id.clone(), tx);
        }
        self.emit_ui_event(ServerMessage::Interaction(
            piko_protocol::InteractionEvent::Requested {
                task_id: request.task_id.clone(),
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
}

fn root_agent_spec(
    cwd: impl AsRef<std::path::Path>,
    system_prompt: String,
    active_tool_names: Option<Vec<String>>,
) -> AgentSpec {
    let mut spec = crate::domain::agents::loader::load_agents(cwd)
        .remove("main")
        .expect("built-in main agent must be registered");
    spec.system_prompt = system_prompt;
    if !spec.tool_set_ids.iter().any(|id| id == "user_interaction") {
        spec.tool_set_ids.push("user_interaction".into());
    }
    spec.active_tool_names = active_tool_names;
    spec
}

#[async_trait]
impl TurnRunner for OrchTurnRunner {
    async fn cancel_work(&self, task_id: &str, work_id: &str) -> Result<bool, ProtocolError> {
        self.runtime
            .agent_runtime()
            .control_task(
                piko_protocol::agent_runtime::TaskControlRequest::CancelWork {
                    request_id: format!("cancel_{}", uuid::Uuid::new_v4()),
                    task_id: task_id.to_string(),
                    work_id: work_id.to_string(),
                },
            )
            .await
            .map(|_| true)
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))
    }

    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, ProtocolError> {
        let gateway_runner = self.with_ui_event_tx(input.ui_event_tx.clone());

        self.runtime
            .set_approval_gateway(Box::new(gateway_runner.clone()))
            .await;
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
        self.runtime
            .register_tool_provider(Box::new(user_provider))
            .await;
        self.runtime
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

        let agent_spec = root_agent_spec(
            &input.cwd,
            input.system_prompt.clone(),
            input.active_tool_names.clone(),
        );
        self.runtime.register_agent(agent_spec.clone()).await;

        let persist_sink = input
            .persist_sink
            .clone()
            .or_else(|| {
                input.session_dir.clone().map(|session_dir| {
                    Arc::new(crate::infra::storage::TaskRepository::new(session_dir))
                        as Arc<dyn PersistSink>
                })
            })
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(
                    "agent runtime requires durable session persistence".into(),
                )
            })?;
        self.runtime.set_persist_sink(persist_sink).await;

        self.runtime
            .agent_runtime()
            .start_root_turn(
                &input.session_id,
                &input.turn_id,
                &input.work_id,
                "main",
                &input.prompt,
                input
                    .resume_root_task
                    .as_ref()
                    .map(|resume| resume.state.clone()),
                input
                    .resume_root_task
                    .as_ref()
                    .map(|resume| resume.task_id.as_str()),
            )
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))
    }

    async fn recover_session_subscription(
        &self,
        session_id: &str,
    ) -> Result<
        (
            piko_protocol::agent_runtime::SessionRuntimeSnapshot,
            SessionSubscription,
        ),
        ProtocolError,
    > {
        let runtime = self.runtime.agent_runtime();
        let snapshot = runtime
            .session_snapshot(session_id.to_string())
            .await
            .map_err(|error| ProtocolError::ObservationFailed(error.to_string()))?;
        let subscription = runtime
            .subscribe_session(piko_protocol::agent_runtime::SubscribeRequest {
                session_id: session_id.to_string(),
                task_id: None,
                after: Some(snapshot.cursor.clone()),
            })
            .await
            .map_err(|error| ProtocolError::ObservationFailed(error.to_string()))?;
        Ok((snapshot, subscription))
    }

    async fn steer_task(
        &self,
        session_id: &str,
        task_id: &str,
        source_task_id: &str,
        source_agent_id: &str,
        message: &str,
    ) -> bool {
        self.runtime
            .agent_runtime()
            .submit_input(build_user_input(
                session_id,
                task_id,
                &format!("work_{}", uuid::Uuid::new_v4()),
                MessageContent::String(message.to_string()),
                InputSource::Task {
                    task_id: source_task_id.to_string(),
                    agent_id: source_agent_id.to_string(),
                },
                None,
            ))
            .await
            .is_ok()
    }

    async fn respond_approval(
        &self,
        approval_id: &str,
        decision: crate::api::ApprovalDecision,
    ) -> Result<bool, ProtocolError> {
        let mut pending = self.pending_approvals.lock().unwrap();
        if let Some(tx) = pending.remove(approval_id) {
            let _ = tx.send(decision);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn respond_user_interaction(
        &self,
        interaction_id: &str,
        response: UserInteractionResponse,
    ) -> Result<bool, ProtocolError> {
        let mut pending = self.pending_interactions.lock().unwrap();
        if let Some(tx) = pending.remove(interaction_id) {
            let _ = tx.send(response);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn on_task_created(&self, task_id: &str, session_id: &str, cwd: &str) {
        self.register_task_context(task_id.to_string(), session_id.to_string(), cwd.to_string());
    }
}

#[async_trait]
impl ApprovalGateway for OrchTurnRunner {
    async fn request_tool_approval(&self, request: ToolApprovalRequest) -> ToolApprovalDecision {
        let _prompt_turn = self.prompt_gate.lock().await;
        let context = self.get_task_context(&request.task_id);
        let cwd = context.as_ref().map(|(_, cwd)| cwd.as_str()).unwrap_or("");

        if !cwd.is_empty() {
            let store = self.get_approval_store(cwd);
            if let Some(scope) = store.is_approved(&request.tool_name, &request.tool_args) {
                tracing::info!(
                    "Auto-accepting pre-approved tool: {} at scope {:?}",
                    request.tool_name,
                    scope
                );
                return match scope {
                    ApprovalScope::Session => ToolApprovalDecision::AcceptSession,
                    ApprovalScope::Workspace => ToolApprovalDecision::AcceptWorkspace,
                    ApprovalScope::Permanent => ToolApprovalDecision::AcceptPermanent,
                };
            }
        }

        let (tx, rx) = oneshot::channel();
        let approval_id = request.tool_entity_id.clone();
        {
            let mut pending = self.pending_approvals.lock().unwrap();
            pending.insert(approval_id.clone(), tx);
        }

        self.emit_ui_event(ServerMessage::Approval(
            crate::api::ApprovalEvent::Requested {
                task_id: request.task_id.clone(),
                agent_id: request.agent_id.clone(),
                approval_id: approval_id.clone(),
                tool_name: request.tool_name.clone(),
                tool_args: request.tool_args.clone(),
            },
        ));

        let decision = match rx.await {
            Ok(d) => d,
            Err(_) => piko_protocol::ApprovalDecision::Decline,
        };

        {
            let mut pending = self.pending_approvals.lock().unwrap();
            pending.remove(&approval_id);
        }

        if !cwd.is_empty() {
            let store = self.get_approval_store(cwd);
            match decision {
                piko_protocol::ApprovalDecision::AcceptSession => {
                    store.grant(
                        &request.tool_name,
                        &request.tool_args,
                        ApprovalScope::Session,
                    );
                }
                piko_protocol::ApprovalDecision::AcceptWorkspace => {
                    store.grant(
                        &request.tool_name,
                        &request.tool_args,
                        ApprovalScope::Workspace,
                    );
                }
                piko_protocol::ApprovalDecision::AcceptPermanent => {
                    store.grant(
                        &request.tool_name,
                        &request.tool_args,
                        ApprovalScope::Permanent,
                    );
                }
                _ => {}
            }
        }

        match decision {
            piko_protocol::ApprovalDecision::Accept => ToolApprovalDecision::Accept,
            piko_protocol::ApprovalDecision::Decline => ToolApprovalDecision::Decline,
            piko_protocol::ApprovalDecision::AcceptSession => ToolApprovalDecision::AcceptSession,
            piko_protocol::ApprovalDecision::AcceptWorkspace => {
                ToolApprovalDecision::AcceptWorkspace
            }
            piko_protocol::ApprovalDecision::AcceptPermanent => {
                ToolApprovalDecision::AcceptPermanent
            }
        }
    }
}
