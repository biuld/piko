use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

use llmd::gateway::LlmGateway;
use orchd::AgentReport;
use orchd::Supervisor;
use orchd::adapters::tools::{
    UserInteractionCallbacks, UserInteractionProvider, UserInteractionRequest,
};
use orchd::domain::tools::approval::{ToolApprovalDecision, ToolApprovalRequest};
use orchd::domain::tools::definition::{ToolSet, ToolSetToolRef};
use orchd::ports::ApprovalGateway;
use orchd::protocol::agents::{AgentSpec, HostTaskContext};
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};
use orchd::runtime::dispatch::SessionChannels;

use crate::api::{ProtocolError, ServerMessage, UserInteractionResponse, UserInteractionStatus};
use crate::domain::config::{McpServerConfig, SandboxSettings};
use crate::domain::turns::approval::{ApprovalScope, ApprovalStore};
use crate::domain::turns::runner::{TurnEventStream, TurnRunInput, TurnRunner};

#[derive(Clone)]
pub struct OrchTurnRunner {
    supervisor: Arc<Supervisor>,
    pending_approvals:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<crate::api::ApprovalDecision>>>>,
    pending_interactions:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<UserInteractionResponse>>>>,
    approval_stores: Arc<std::sync::Mutex<HashMap<String, Arc<ApprovalStore>>>>,
    task_contexts: Arc<std::sync::Mutex<HashMap<String, (String, String)>>>, // task_id -> (session_id, cwd)
    turn_event_tx: Option<UnboundedSender<ServerMessage>>,
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
        use orchd::protocol::config::{ModelRef, OrchdConfig, ProviderConfig, SandboxConfig};
        use orchd::protocol::model::ModelRunSettings;

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

        let config = OrchdConfig {
            providers,
            agents: Default::default(),
            default_model: ModelRef {
                provider: provider.to_string(),
                model_id: model_id.to_string(),
            },
            default_settings,
            runtime: Default::default(),
            thinking_level_map,
            sandbox,
        };
        let supervisor = Supervisor::from_config(model_executor, config).await;

        // Initialize MCP tools
        let registry = supervisor.tool_registry().clone();
        let registered = crate::infra::mcp::initialize_mcp_tools(mcp_configs, registry).await;
        if !registered.is_empty() {
            tracing::info!("MCP tools registered: {:?}", registered);
        }

        Self {
            supervisor,
            pending_approvals: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_interactions: Arc::new(std::sync::Mutex::new(HashMap::new())),
            approval_stores: Arc::new(std::sync::Mutex::new(HashMap::new())),
            task_contexts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            turn_event_tx: None,
            prompt_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn with_turn_event_tx(&self, turn_event_tx: UnboundedSender<ServerMessage>) -> Self {
        Self {
            supervisor: Arc::clone(&self.supervisor),
            pending_approvals: Arc::clone(&self.pending_approvals),
            pending_interactions: Arc::clone(&self.pending_interactions),
            approval_stores: Arc::clone(&self.approval_stores),
            task_contexts: Arc::clone(&self.task_contexts),
            turn_event_tx: Some(turn_event_tx),
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

    async fn request_user_interaction(
        &self,
        request: UserInteractionRequest,
    ) -> UserInteractionResponse {
        let _prompt_turn = self.prompt_gate.lock().await;
        let Some(event_tx) = self.turn_event_tx.as_ref().cloned() else {
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
        let _ = event_tx.send(ServerMessage::Display(
            piko_protocol::DisplayEvent::InteractionEvent(
                crate::api::InteractionEvent::Requested {
                    task_id: request.task_id.clone(),
                    agent_id: request.agent_id.clone(),
                    interaction_id: interaction_id.clone(),
                    tool_call_id: request.tool_call_id,
                    title: request.title,
                    questions: request.questions,
                    require_confirm: request.require_confirm,
                    auto_resolution_ms: request.auto_resolution_ms,
                },
            ),
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
        let status = match response {
            UserInteractionResponse::Submit { .. } => UserInteractionStatus::Submitted,
            UserInteractionResponse::Cancel { .. } => UserInteractionStatus::Cancelled,
        };
        let _ = event_tx.send(ServerMessage::Display(
            piko_protocol::DisplayEvent::InteractionEvent(crate::api::InteractionEvent::Resolved {
                task_id: request.task_id,
                agent_id: request.agent_id,
                interaction_id,
                status,
            }),
        ));
        response
    }
}

#[async_trait]
impl TurnRunner for OrchTurnRunner {
    async fn run_turn(&self, input: TurnRunInput) -> Result<TurnEventStream, ProtocolError> {
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        let agent_id = "main".to_string();
        let cwd = input.cwd.clone();
        let (side_tx, side_rx) = unbounded_channel::<ServerMessage>();
        let gateway_runner = self.with_turn_event_tx(side_tx.clone());

        // Register approval gateway on tool registry
        let registry = self.supervisor.tool_registry().clone();
        registry
            .set_approval_gateway(Some(Box::new(gateway_runner.clone())))
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
        registry.register_provider(Box::new(user_provider)).await;
        registry
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

        // Register agent
        let agent_spec = AgentSpec {
            id: agent_id.clone(),
            name: agent_id.clone(),
            role: "assistant".into(),
            description: Some("hostd-managed agent".into()),
            system_prompt: input.system_prompt.clone(),
            model: None,
            tool_set_ids: vec![
                "builtin".into(),
                "workspace".into(),
                "user_interaction".into(),
            ],
            active_tool_names: input.active_tool_names.clone(),
            thinking_level: None,
        };
        self.supervisor.register_agent(agent_spec.clone()).await;

        // Start root agent stream
        let root_stream = self
            .supervisor
            .run_streaming(
                &input.prompt,
                Some(OrchRunOptions {
                    command: OrchRunCommandOptions {
                        target_agent_id: Some(agent_id.clone()),
                    },
                    history: None,
                    host_context: Some(HostTaskContext {
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                    }),
                }),
            )
            .await;

        let mut stream = root_stream;
        let mut side_stream = UnboundedReceiverStream::new(side_rx);
        let runner = self.clone();
        let supervisor = Arc::clone(&self.supervisor);
        let stream_agent_id = agent_id.clone();

        Ok(Box::pin(async_stream::try_stream! {
            let mut root_done = false;
            let mut side_done = false;
            drop(side_tx);

            while !root_done {
                tokio::select! {
                    root_event = stream.next() => {
                        match root_event {
                            Some(event) => {
                                runner.observe_turn_event(&event, &cwd).await;
                                yield event;
                            }
                            None => root_done = true,
                        }
                    }
                    side_event = side_stream.next(), if !side_done => {
                        match side_event {
                            Some(event) => yield event,
                            None => side_done = true,
                        }
                    }
                }
            }

            while let Ok(Some(event)) = tokio::time::timeout(
                std::time::Duration::from_millis(1),
                side_stream.next(),
            )
            .await
            {
                yield event;
            }

            supervisor.unregister_agent(&stream_agent_id).await;
        }))
    }

    async fn run_turn_channels(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionChannels, ProtocolError> {
        let (side_tx, side_rx) = unbounded_channel::<ServerMessage>();
        let gateway_runner = self.with_turn_event_tx(side_tx.clone());

        let registry = self.supervisor.tool_registry().clone();
        registry
            .set_approval_gateway(Some(Box::new(gateway_runner.clone())))
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
        registry.register_provider(Box::new(user_provider)).await;
        registry
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

        let agent_spec = AgentSpec {
            id: "main".to_string(),
            name: "main".to_string(),
            role: "assistant".into(),
            description: Some("hostd-managed agent".into()),
            system_prompt: input.system_prompt.clone(),
            model: None,
            tool_set_ids: vec![
                "builtin".into(),
                "workspace".into(),
                "user_interaction".into(),
            ],
            active_tool_names: input.active_tool_names.clone(),
            thinking_level: None,
        };
        self.supervisor.register_agent(agent_spec.clone()).await;

        let channels = self
            .supervisor
            .run_streaming_channels(
                &input.prompt,
                Some(OrchRunOptions {
                    command: OrchRunCommandOptions {
                        target_agent_id: Some("main".into()),
                    },
                    history: None,
                    host_context: Some(HostTaskContext {
                        session_id: input.session_id.clone(),
                        turn_id: input.turn_id.clone(),
                    }),
                }),
            )
            .await;

        // Forward side events (approvals, interactions) to display channel
        let display_tx = channels.display_sender();
        tokio::spawn(async move {
            let mut side_stream = UnboundedReceiverStream::new(side_rx);
            while let Some(event) = side_stream.next().await {
                use orchd::runtime::dispatch::display_events_from_server_message;
                for display in display_events_from_server_message(&event) {
                    let _ = display_tx.send(display).await;
                }
            }
        });

        Ok(channels)
    }

    async fn steer_task(
        &self,
        task_id: &str,
        _source_task_id: &str,
        _source_agent_id: &str,
        message: &str,
    ) -> bool {
        self.supervisor
            .to_spawner()
            .steer_task(task_id, message)
            .await
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
}

impl OrchTurnRunner {
    async fn observe_turn_event(&self, event: &ServerMessage, cwd: &str) {
        if let ServerMessage::Display(piko_protocol::DisplayEvent::TaskLifecycle(
            crate::api::TaskEvent::Created {
                task_id,
                session_id,
                ..
            },
        )) = event
        {
            self.register_task_context(task_id.clone(), session_id.clone(), cwd.to_string());
        }

        match event {
            ServerMessage::Display(piko_protocol::DisplayEvent::TaskLifecycle(
                crate::api::TaskEvent::Completed {
                    task_id,
                    summary,
                    final_status,
                    total_steps,
                    ..
                },
            )) => {
                self.supervisor
                    .record_task_result(
                        task_id,
                        AgentReport {
                            text: summary.clone(),
                            status: final_status.clone(),
                            total_steps: *total_steps,
                            task_id: None,
                        },
                    )
                    .await;
            }
            ServerMessage::Display(piko_protocol::DisplayEvent::TaskLifecycle(
                crate::api::TaskEvent::Failed { task_id, error, .. },
            )) => {
                self.supervisor
                    .record_task_result(
                        task_id,
                        AgentReport {
                            text: error.clone(),
                            status: "error".into(),
                            total_steps: 0,
                            task_id: None,
                        },
                    )
                    .await;
            }
            ServerMessage::Display(piko_protocol::DisplayEvent::TaskLifecycle(
                crate::api::TaskEvent::Cancelled { task_id, .. },
            )) => {
                self.supervisor
                    .record_task_result(
                        task_id,
                        AgentReport {
                            text: "cancelled".into(),
                            status: "cancelled".into(),
                            total_steps: 0,
                            task_id: None,
                        },
                    )
                    .await;
            }
            _ => {}
        }
    }
}

#[async_trait]
impl ApprovalGateway for OrchTurnRunner {
    async fn request_tool_approval(&self, request: ToolApprovalRequest) -> ToolApprovalDecision {
        let _prompt_turn = self.prompt_gate.lock().await;
        // 1. Resolve cwd for this task
        let context = self.get_task_context(&request.task_id);
        let cwd = context.as_ref().map(|(_, cwd)| cwd.as_str()).unwrap_or("");

        // 2. Check if pre-approved!
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

        // 3. Not pre-approved, register a pending oneshot channel
        let (tx, rx) = oneshot::channel();
        let approval_id = request.tool_entity_id.clone();
        {
            let mut pending = self.pending_approvals.lock().unwrap();
            pending.insert(approval_id.clone(), tx);
        }

        if let Some(event_tx) = self.turn_event_tx.as_ref() {
            let _ = event_tx.send(ServerMessage::Approval(
                crate::api::ApprovalEvent::Requested {
                    task_id: request.task_id.clone(),
                    agent_id: request.agent_id.clone(),
                    approval_id: approval_id.clone(),
                    tool_name: request.tool_name.clone(),
                    tool_args: request.tool_args.clone(),
                },
            ));
        }

        // 4. Wait for user response
        let decision = match rx.await {
            Ok(d) => d,
            Err(_) => {
                // Sender dropped (e.g. timeout / shutdown)
                piko_protocol::ApprovalDecision::Decline
            }
        };

        // Clean up from pending approvals list
        {
            let mut pending = self.pending_approvals.lock().unwrap();
            pending.remove(&approval_id);
        }

        // 5. If approved with a wider scope, save to ApprovalStore
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

        let host_decision = decision.clone();
        if let Some(event_tx) = self.turn_event_tx.as_ref() {
            let _ = event_tx.send(ServerMessage::Approval(
                crate::api::ApprovalEvent::Resolved {
                    task_id: request.task_id.clone(),
                    agent_id: request.agent_id.clone(),
                    approval_id,
                    decision: host_decision.clone(),
                },
            ));
        }

        // 6. Map and return
        match host_decision {
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
