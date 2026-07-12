use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use llmd::gateway::LlmGateway;
use orchd::AgentExecutionRuntime;
use orchd::tools::{UserInteractionCallbacks, UserInteractionProvider, UserInteractionRequest};
use orchd_api::{
    AgentExecutor, ApprovalGateway, CancelExecutionRequest, CancelReason, ConversationContext,
    ExecutionCommitPort, ExecutionConfig, PersistSink, SessionExecutionConfig,
    SessionExecutionPorts, SessionSubscription, StartExecutionRequest, SteerExecutionRequest,
    ToolApprovalDecision, ToolApprovalRequest,
};
use piko_protocol::MessageContent;
use piko_protocol::agents::AgentSpec;
use piko_protocol::tools::{ToolSet, ToolSetToolRef};

use crate::api::{ProtocolError, ServerMessage, UserInteractionResponse};
use crate::domain::config::{McpServerConfig, SandboxSettings};
use crate::domain::turns::approval::{ApprovalScope, ApprovalStore};
use crate::domain::turns::legacy_execution_commit::LegacyPersistExecutionCommitPort;
use crate::domain::turns::notifying_execution_commit::NotifyingExecutionCommitPort;
use crate::domain::turns::runner::{TurnRunInput, TurnRunner};

#[derive(Clone)]
pub struct OrchTurnRunner {
    execution: Arc<AgentExecutionRuntime>,
    /// session_id -> (turn_id, execution_id) for the Execution-runtime path.
    active_executions: Arc<std::sync::Mutex<HashMap<String, (String, String)>>>,
    /// Live observation hubs for Execution turns (reconnect without cancelling).
    active_hubs: Arc<std::sync::Mutex<HashMap<String, Arc<orchd::testing::SessionOutputHub>>>>,
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
        let execution = AgentExecutionRuntime::bootstrap(model_executor, config).await;

        let registered =
            crate::infra::mcp::initialize_mcp_tools(mcp_configs, execution.as_ref()).await;
        if !registered.is_empty() {
            tracing::info!("MCP tools registered: {:?}", registered);
        }

        Self {
            execution,
            active_executions: Arc::new(std::sync::Mutex::new(HashMap::new())),
            active_hubs: Arc::new(std::sync::Mutex::new(HashMap::new())),
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
            execution: Arc::clone(&self.execution),
            active_executions: Arc::clone(&self.active_executions),
            active_hubs: Arc::clone(&self.active_hubs),
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
        self.execution
            .register_tool_provider(Box::new(user_provider))
            .await;
        self.execution
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

    async fn run_execution_turn_subscription(
        &self,
        input: TurnRunInput,
        persist_sink: Arc<dyn PersistSink>,
        agent_spec: AgentSpec,
    ) -> Result<SessionSubscription, ProtocolError> {
        let execution_id = format!("exec_{}", uuid::Uuid::new_v4());
        let input_message_id = format!("msg_user_{}", uuid::Uuid::new_v4());
        let hub = Arc::new(orchd::testing::SessionOutputHub::new(
            input.session_id.clone(),
            uuid::Uuid::new_v4().to_string(),
            64,
        ));
        let legacy = Arc::new(LegacyPersistExecutionCommitPort::new(
            Arc::clone(&persist_sink),
            agent_spec.id.clone(),
        ));
        legacy
            .ensure_execution_shard(&input.session_id, &execution_id, &input.turn_id)
            .await
            .map_err(|err| ProtocolError::InvalidCommand(err.to_string()))?;

        let commit: Arc<dyn ExecutionCommitPort> = Arc::new(NotifyingExecutionCommitPort::new(
            legacy as Arc<dyn ExecutionCommitPort>,
            Arc::clone(&hub),
            agent_spec.id.clone(),
        ));

        // User message is committed by host before start_execution.
        commit
            .commit_message(piko_protocol::execution::MessageCommit {
                session_id: input.session_id.clone(),
                turn_id: input.turn_id.clone(),
                execution_id: execution_id.clone(),
                message_id: input_message_id.clone(),
                parent_message_id: input
                    .resume_root_task
                    .as_ref()
                    .and_then(|resume| resume.state.head_message_id.clone()),
                message: piko_protocol::Message::User {
                    content: MessageContent::String(input.prompt.clone()),
                    timestamp: Some(chrono::Utc::now().timestamp_millis()),
                },
                committed_at: chrono::Utc::now().timestamp_millis(),
            })
            .await
            .map_err(|err| ProtocolError::InvalidCommand(err.to_string()))?;

        // Agent already registered by caller for the execution path.
        let _ = self
            .execution
            .detach_session(input.session_id.clone())
            .await;
        self.execution
            .attach_session(SessionExecutionConfig {
                session_id: input.session_id.clone(),
                ports: SessionExecutionPorts::new(Arc::clone(&commit)),
            })
            .await
            .map_err(|err| ProtocolError::InvalidCommand(err.to_string()))?;

        let context = ConversationContext {
            messages: input
                .resume_root_task
                .as_ref()
                .map(|resume| resume.state.transcript.clone())
                .unwrap_or_default(),
            head_message_id: input
                .resume_root_task
                .as_ref()
                .and_then(|resume| resume.state.head_message_id.clone()),
            system_prompt: Some(input.system_prompt.clone()),
        };

        let receipt = self
            .execution
            .start_execution(StartExecutionRequest {
                request_id: format!("req_{}", uuid::Uuid::new_v4()),
                session_id: input.session_id.clone(),
                turn_id: input.turn_id.clone(),
                execution_id: execution_id.clone(),
                input_message_id,
                input: MessageContent::String(input.prompt.clone()),
                context,
                config: ExecutionConfig {
                    agent_id: agent_spec.id.clone(),
                    model: None,
                    provider: None,
                    allow_tool_calls: true,
                },
            })
            .await
            .map_err(|err| ProtocolError::InvalidCommand(err.to_string()))?;

        tracing::info!(
            session_id = %input.session_id,
            turn_id = %input.turn_id,
            execution_id = %receipt.execution_id,
            "execution runtime path started"
        );

        {
            let mut active = self.active_executions.lock().unwrap();
            active.insert(
                input.session_id.clone(),
                (input.turn_id.clone(), receipt.execution_id.clone()),
            );
        }
        {
            let mut hubs = self.active_hubs.lock().unwrap();
            hubs.insert(input.session_id.clone(), Arc::clone(&hub));
        }

        let cursor = hub.cursor();
        let hub_sub = hub
            .subscribe(&piko_protocol::agent_runtime::SessionCursor {
                epoch: cursor.epoch.clone(),
                seq: 0,
            })
            .await
            .map_err(|reason| ProtocolError::ObservationFailed(reason.to_string()))?;

        // Publish Running so hostd observation can bind root_execution_id.
        let _ = hub
            .publish_event(piko_protocol::agent_runtime::SessionEventEnvelope {
                task_id: execution_id.clone(),
                agent_id: agent_spec.id.clone(),
                task_seq: 0,
                cursor: hub.cursor(),
                event: piko_protocol::agent_runtime::SessionEvent::ExecutionChanged {
                    snapshot: piko_protocol::ExecutionObservationSnapshot {
                        session_id: input.session_id.clone(),
                        turn_id: input.turn_id.clone(),
                        execution_id: execution_id.clone(),
                        agent_id: agent_spec.id.clone(),
                        status: piko_protocol::ExecutionStatus::Running,
                    },
                },
            })
            .await;

        let execution = Arc::clone(&self.execution);
        let active_executions = Arc::clone(&self.active_executions);
        let active_hubs = Arc::clone(&self.active_hubs);
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        let agent_id = agent_spec.id.clone();
        let hub_for_terminal = Arc::clone(&hub);
        tokio::spawn(async move {
            let outcome = execution.wait_terminal(&session_id, &execution_id).await;
            {
                let mut active = active_executions.lock().unwrap();
                if active
                    .get(&session_id)
                    .is_some_and(|(_, id)| id == &execution_id)
                {
                    active.remove(&session_id);
                }
            }
            let status = match outcome {
                Ok(piko_protocol::execution::ExecutionOutcome::Succeeded { .. }) => {
                    piko_protocol::ExecutionStatus::Succeeded
                }
                Ok(piko_protocol::execution::ExecutionOutcome::Cancelled { .. }) => {
                    piko_protocol::ExecutionStatus::Cancelled
                }
                _ => piko_protocol::ExecutionStatus::Failed,
            };
            let _ = hub_for_terminal
                .publish_event(piko_protocol::agent_runtime::SessionEventEnvelope {
                    task_id: execution_id.clone(),
                    agent_id: agent_id.clone(),
                    task_seq: 0,
                    cursor: hub_for_terminal.cursor(),
                    event: piko_protocol::agent_runtime::SessionEvent::ExecutionChanged {
                        snapshot: piko_protocol::ExecutionObservationSnapshot {
                            session_id: session_id.clone(),
                            turn_id,
                            execution_id: execution_id.clone(),
                            agent_id,
                            status,
                        },
                    },
                })
                .await;
            let mut hubs = active_hubs.lock().unwrap();
            hubs.remove(&session_id);
        });

        Ok(SessionSubscription {
            session_id: input.session_id,
            cursor: cursor.clone(),
            output: orchd::testing::merged_output_stream(hub_sub, cursor, None),
        })
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
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, ProtocolError> {
        let gateway_runner = self.with_ui_event_tx(input.ui_event_tx.clone());

        self.execution
            .set_approval_gateway(Box::new(gateway_runner.clone()))
            .await;

        let agent_spec = root_agent_spec(
            &input.cwd,
            input.system_prompt.clone(),
            input.active_tool_names.clone(),
        );

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

        self.register_user_interaction_tools_on_execution(&gateway_runner)
            .await;
        self.execution.register_agent(agent_spec.clone()).await;
        tracing::info!(
            session_id = %input.session_id,
            turn_id = %input.turn_id,
            work_id = %input.work_id,
            "turn subscription starting; dispatching execution runtime"
        );
        self.run_execution_turn_subscription(input, persist_sink, agent_spec)
            .await
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
        // Resubscribe the live hub without cancelling the Execution Actor.
        let hub = {
            let hubs = self.active_hubs.lock().unwrap();
            hubs.get(session_id).cloned()
        };
        let Some(hub) = hub else {
            return Err(ProtocolError::ObservationFailed(format!(
                "no live execution observation hub for session {session_id}"
            )));
        };
        let execution_id = {
            let active = self.active_executions.lock().unwrap();
            active.get(session_id).map(|(_, id)| id.clone())
        };
        let cursor = hub.cursor();
        let hub_sub = hub
            .subscribe(&cursor)
            .await
            .map_err(|reason| ProtocolError::ObservationFailed(reason.to_string()))?;
        let snapshot = piko_protocol::agent_runtime::SessionRuntimeSnapshot {
            session_id: session_id.to_string(),
            root_task_id: execution_id.clone(),
            active_task_id: execution_id,
            tasks: Vec::new(),
            cursor: cursor.clone(),
        };
        Ok((
            snapshot,
            SessionSubscription {
                session_id: session_id.to_string(),
                cursor: cursor.clone(),
                output: orchd::testing::merged_output_stream(hub_sub, cursor, None),
            },
        ))
    }

    async fn steer_task(
        &self,
        session_id: &str,
        _task_id: &str,
        _source_task_id: &str,
        _source_agent_id: &str,
        message: &str,
    ) -> bool {
        let execution_id = self
            .active_executions
            .lock()
            .unwrap()
            .get(session_id)
            .map(|(_, execution_id)| execution_id.clone());
        let Some(execution_id) = execution_id else {
            return false;
        };
        self.execution
            .steer_execution(SteerExecutionRequest {
                request_id: format!("req_steer_{}", uuid::Uuid::new_v4()),
                session_id: session_id.to_string(),
                execution_id,
                message_id: format!("msg_steer_{}", uuid::Uuid::new_v4()),
                content: MessageContent::String(message.to_string()),
                submitted_at: chrono::Utc::now().timestamp_millis(),
            })
            .await
            .is_ok()
    }

    async fn cancel_execution(&self, session_id: &str, turn_id: &str) -> bool {
        let execution_id = {
            let active = self.active_executions.lock().unwrap();
            active
                .get(session_id)
                .filter(|(active_turn, _)| active_turn == turn_id)
                .map(|(_, execution_id)| execution_id.clone())
        };
        let Some(execution_id) = execution_id else {
            return false;
        };
        self.execution
            .request_cancel(CancelExecutionRequest {
                request_id: format!("req_cancel_{}", uuid::Uuid::new_v4()),
                session_id: session_id.to_string(),
                execution_id,
                reason: CancelReason::UserRequested,
            })
            .await
            .map(|receipt| receipt.accepted)
            .unwrap_or(false)
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
