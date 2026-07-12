use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use llmd::gateway::LlmGateway;
use orchd::AgentRuntime;
use orchd::tools::{UserInteractionCallbacks, UserInteractionProvider, UserInteractionRequest};
use orchd_api::{
    AgentCommitPort, AgentRecoveryState, AgentRuntimeApi, ApprovalGateway, CancelExecutionRequest,
    CancelReason, ExecutionCommitPort, PersistSink, RealtimeDeltaSink, SessionAgentConfig,
    SessionAgentPorts, SessionExecutionPorts, SessionSubscription, ToolApprovalDecision,
    ToolApprovalRequest,
};
use piko_protocol::agents::AgentSpec;
use piko_protocol::tools::{ToolSet, ToolSetToolRef};
use piko_protocol::{
    AgentCommitAck, AgentDurableCommand, AgentInputDelivery, AgentInstanceIdentity,
    AgentInstanceLifecycle, CommitAck, CommitError, ExecutionOutcomeCommit, MessageContent,
    SendAgentInputRequest,
};

use crate::api::{ProtocolError, ServerMessage, UserInteractionResponse};
use crate::domain::config::{McpServerConfig, SandboxSettings};
use crate::domain::turns::approval::{ApprovalScope, ApprovalStore};
use crate::domain::turns::legacy_execution_commit::LegacyPersistExecutionCommitPort;
use crate::domain::turns::notifying_execution_commit::NotifyingExecutionCommitPort;
use crate::domain::turns::runner::{TurnRunInput, TurnRunner};
use crate::infra::storage::task_repository::{
    SESSION_SCHEMA_VERSION, TaskRepository, TaskShardHeader,
};

struct ExecutionCommitRouter {
    routes: std::sync::Mutex<HashMap<String, Arc<dyn ExecutionCommitPort>>>,
    fallback: Option<Arc<dyn ExecutionCommitPort>>,
}

#[derive(Default)]
struct RealtimeDeltaRouter {
    hubs: std::sync::Mutex<HashMap<String, Arc<orchd::testing::SessionOutputHub>>>,
    default_hub: std::sync::Mutex<Option<Arc<orchd::testing::SessionOutputHub>>>,
}

impl RealtimeDeltaRouter {
    fn register(&self, execution_id: String, hub: Arc<orchd::testing::SessionOutputHub>) {
        self.hubs
            .lock()
            .unwrap()
            .insert(execution_id, Arc::clone(&hub));
        *self.default_hub.lock().unwrap() = Some(hub);
    }
}

impl RealtimeDeltaSink for RealtimeDeltaRouter {
    fn try_publish(&self, delta: piko_protocol::agent_runtime::RealtimeDeltaEnvelope) {
        let hub = self
            .hubs
            .lock()
            .unwrap()
            .get(&delta.work_id)
            .cloned()
            .or_else(|| self.default_hub.lock().unwrap().clone());
        if let Some(hub) = hub {
            hub.try_publish_delta(delta);
        }
    }
}

impl ExecutionCommitRouter {
    fn new(fallback: Option<Arc<dyn ExecutionCommitPort>>) -> Self {
        Self {
            routes: std::sync::Mutex::new(HashMap::new()),
            fallback,
        }
    }

    fn register(&self, execution_id: String, port: Arc<dyn ExecutionCommitPort>) {
        self.routes.lock().unwrap().insert(execution_id, port);
    }

    fn route(&self, execution_id: &str) -> Option<Arc<dyn ExecutionCommitPort>> {
        self.routes
            .lock()
            .unwrap()
            .get(execution_id)
            .cloned()
            .or_else(|| self.fallback.clone())
    }
}

#[async_trait]
impl ExecutionCommitPort for ExecutionCommitRouter {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<CommitAck, CommitError> {
        self.route(&commit.execution_id)
            .ok_or(CommitError::Unavailable)?
            .commit_message(commit)
            .await
    }

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError> {
        self.route(&commit.execution_id)
            .ok_or(CommitError::Unavailable)?
            .commit_execution_outcome(commit)
            .await
    }
}

struct RepositoryExecutionCommitPort {
    repository: TaskRepository,
}

#[async_trait]
impl ExecutionCommitPort for RepositoryExecutionCommitPort {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<CommitAck, CommitError> {
        let manifest = self
            .repository
            .load_manifest()
            .map_err(|error| CommitError::Failed(error.to_string()))?;
        let agent_id = manifest
            .agents
            .get(&commit.agent_instance_id)
            .map(|agent| agent.identity.agent_spec_id.clone())
            .ok_or(CommitError::IdentityMismatch)?;
        if self
            .repository
            .load_task(&commit.session_id, &commit.execution_id)
            .is_err()
        {
            self.repository
                .create_task(TaskShardHeader {
                    schema_version: SESSION_SCHEMA_VERSION,
                    session_id: commit.session_id.clone(),
                    task_id: commit.execution_id.clone(),
                    agent_id: agent_id.clone(),
                    agent_instance_id: Some(commit.agent_instance_id.clone()),
                    parent_task_id: None,
                    created_at: commit.committed_at,
                })
                .map_err(map_persist_error)?;
        }
        let recovered = self
            .repository
            .load_task(&commit.session_id, &commit.execution_id)
            .map_err(|error| CommitError::Failed(error.to_string()))?;
        let revision = recovered.last_task_seq.saturating_add(1);
        self.repository
            .commit_message(orchd_api::MessageCommit {
                session_id: commit.session_id.clone(),
                task_id: commit.execution_id.clone(),
                agent_id,
                agent_instance_id: Some(commit.agent_instance_id.clone()),
                work_id: commit.turn_id.clone(),
                task_seq: revision,
                message_id: commit.message_id.clone(),
                parent_message_id: recovered.head_message_id,
                message: commit.message,
                committed_at: commit.committed_at,
            })
            .map_err(map_persist_error)?;
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision,
        })
    }

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError> {
        let revision = self
            .repository
            .load_task(&commit.session_id, &commit.execution_id)
            .map(|task| task.last_task_seq)
            .unwrap_or_default();
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: None,
            revision,
        })
    }
}

#[derive(Default)]
struct EphemeralAgentCommitPort {
    revision: AtomicU64,
}

struct ProjectingAgentCommitPort {
    inner: Arc<dyn AgentCommitPort>,
    agents: std::sync::Mutex<HashMap<String, crate::api::AgentInfo>>,
    event_tx: Arc<std::sync::Mutex<Option<UnboundedSender<ServerMessage>>>>,
}

impl ProjectingAgentCommitPort {
    fn new(
        inner: Arc<dyn AgentCommitPort>,
        recovered: &[AgentRecoveryState],
        event_tx: Arc<std::sync::Mutex<Option<UnboundedSender<ServerMessage>>>>,
    ) -> Self {
        let agents = recovered
            .iter()
            .map(|state| {
                let status = lifecycle_status(state.lifecycle);
                (
                    state.identity.agent_instance_id.clone(),
                    crate::api::AgentInfo {
                        agent_instance_id: state.identity.agent_instance_id.clone(),
                        agent_id: state.identity.agent_spec_id.clone(),
                        parent_agent_instance_id: state.identity.parent_agent_instance_id.clone(),
                        lifecycle: state.lifecycle,
                        activity: piko_protocol::AgentActivity::Idle,
                        unread_report_count: state
                            .inbox
                            .iter()
                            .filter(|item| item.consumed_at.is_none())
                            .count() as u32,
                        task_id: state.identity.agent_instance_id.clone(),
                        parent_task_id: state.identity.parent_agent_instance_id.clone(),
                        name: state.spec.name.clone(),
                        role: state.spec.role.clone(),
                        status,
                    },
                )
            })
            .collect();
        Self {
            inner,
            agents: std::sync::Mutex::new(agents),
            event_tx,
        }
    }

    fn project(&self, command: AgentDurableCommand) {
        let changed = {
            let mut agents = self.agents.lock().unwrap();
            match command {
                AgentDurableCommand::Create { identity, spec } => {
                    let info = crate::api::AgentInfo {
                        agent_instance_id: identity.agent_instance_id.clone(),
                        agent_id: identity.agent_spec_id,
                        parent_agent_instance_id: identity.parent_agent_instance_id.clone(),
                        lifecycle: AgentInstanceLifecycle::Open,
                        activity: piko_protocol::AgentActivity::Idle,
                        unread_report_count: 0,
                        task_id: identity.agent_instance_id.clone(),
                        parent_task_id: identity.parent_agent_instance_id,
                        name: spec.name,
                        role: spec.role,
                        status: crate::api::AgentStatus::Idle,
                    };
                    agents.insert(identity.agent_instance_id, info.clone());
                    Some(info)
                }
                AgentDurableCommand::ExecutionStarted {
                    agent_instance_id,
                    execution_id,
                    ..
                } => agents.get_mut(&agent_instance_id).map(|info| {
                    info.activity = piko_protocol::AgentActivity::Running { execution_id };
                    info.status = crate::api::AgentStatus::Running;
                    info.clone()
                }),
                AgentDurableCommand::RecordExecutionReport { report } => {
                    agents.get_mut(&report.agent_instance_id).map(|info| {
                        info.activity = piko_protocol::AgentActivity::Idle;
                        info.status = crate::api::AgentStatus::Idle;
                        info.clone()
                    })
                }
                AgentDurableCommand::SetLifecycle {
                    agent_instance_id,
                    lifecycle,
                } => agents.get_mut(&agent_instance_id).map(|info| {
                    info.lifecycle = lifecycle;
                    info.status = lifecycle_status(lifecycle);
                    info.clone()
                }),
                AgentDurableCommand::CommitReport {
                    recipient_agent_instance_id,
                    ..
                } => agents.get_mut(&recipient_agent_instance_id).map(|info| {
                    info.unread_report_count = info.unread_report_count.saturating_add(1);
                    info.clone()
                }),
                AgentDurableCommand::ConsumeInboxItem {
                    agent_instance_id, ..
                } => agents.get_mut(&agent_instance_id).map(|info| {
                    info.unread_report_count = info.unread_report_count.saturating_sub(1);
                    info.clone()
                }),
            }
        };
        if let Some(changed) = changed
            && let Some(tx) = self.event_tx.lock().unwrap().as_ref()
        {
            let _ = tx.send(ServerMessage::AgentChanged(changed));
        }
    }
}

#[async_trait]
impl AgentCommitPort for ProjectingAgentCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let ack = self
            .inner
            .commit_agent_command(session_id, command.clone())
            .await?;
        self.project(command);
        Ok(ack)
    }
}

fn lifecycle_status(lifecycle: AgentInstanceLifecycle) -> crate::api::AgentStatus {
    match lifecycle {
        AgentInstanceLifecycle::Open => crate::api::AgentStatus::Idle,
        AgentInstanceLifecycle::Closed => crate::api::AgentStatus::Closed,
        AgentInstanceLifecycle::Terminated => crate::api::AgentStatus::Stopped,
        AgentInstanceLifecycle::Unavailable => crate::api::AgentStatus::Failed,
    }
}

fn map_persist_error(error: orchd_api::PersistError) -> CommitError {
    match error {
        orchd_api::PersistError::Unavailable => CommitError::Unavailable,
        orchd_api::PersistError::IdentityMismatch => CommitError::IdentityMismatch,
        orchd_api::PersistError::SequenceMismatch { expected, actual } => {
            CommitError::SequenceMismatch { expected, actual }
        }
        orchd_api::PersistError::IdempotencyConflict => CommitError::IdempotencyConflict,
        orchd_api::PersistError::Failed(message) => CommitError::Failed(message),
    }
}

#[async_trait]
impl AgentCommitPort for EphemeralAgentCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let agent_instance_id = match command {
            AgentDurableCommand::Create { identity, .. } => identity.agent_instance_id,
            AgentDurableCommand::SetLifecycle {
                agent_instance_id, ..
            }
            | AgentDurableCommand::ConsumeInboxItem {
                agent_instance_id, ..
            } => agent_instance_id,
            AgentDurableCommand::RecordExecutionReport { report } => report.agent_instance_id,
            AgentDurableCommand::ExecutionStarted {
                agent_instance_id, ..
            } => agent_instance_id,
            AgentDurableCommand::CommitReport {
                recipient_agent_instance_id,
                ..
            } => recipient_agent_instance_id,
        };
        Ok(AgentCommitAck {
            session_id: session_id.into(),
            agent_instance_id,
            revision: self.revision.fetch_add(1, Ordering::SeqCst) + 1,
        })
    }
}

#[derive(Clone)]
pub struct OrchTurnRunner {
    agent_runtime: Arc<AgentRuntime>,
    /// session_id -> (turn_id, execution_id) for the Execution-runtime path.
    active_executions: Arc<std::sync::Mutex<HashMap<String, (String, String)>>>,
    /// Live observation hubs for Execution turns (reconnect without cancelling).
    active_hubs: Arc<std::sync::Mutex<HashMap<String, Arc<orchd::testing::SessionOutputHub>>>>,
    commit_routers: Arc<std::sync::Mutex<HashMap<String, Arc<ExecutionCommitRouter>>>>,
    realtime_routers: Arc<std::sync::Mutex<HashMap<String, Arc<RealtimeDeltaRouter>>>>,
    pending_approvals:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<crate::api::ApprovalDecision>>>>,
    pending_interactions:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<UserInteractionResponse>>>>,
    approval_stores: Arc<std::sync::Mutex<HashMap<String, Arc<ApprovalStore>>>>,
    task_contexts: Arc<std::sync::Mutex<HashMap<String, (String, String)>>>, // task_id -> (session_id, cwd)
    agent_event_tx: Arc<std::sync::Mutex<Option<UnboundedSender<ServerMessage>>>>,
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
        let agent_runtime = AgentRuntime::bootstrap(model_executor, config).await;

        let registered =
            crate::infra::mcp::initialize_mcp_tools(mcp_configs, agent_runtime.as_ref()).await;
        if !registered.is_empty() {
            tracing::info!("MCP tools registered: {:?}", registered);
        }

        Self {
            agent_runtime,
            active_executions: Arc::new(std::sync::Mutex::new(HashMap::new())),
            active_hubs: Arc::new(std::sync::Mutex::new(HashMap::new())),
            commit_routers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            realtime_routers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_approvals: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_interactions: Arc::new(std::sync::Mutex::new(HashMap::new())),
            approval_stores: Arc::new(std::sync::Mutex::new(HashMap::new())),
            task_contexts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            agent_event_tx: Arc::new(std::sync::Mutex::new(None)),
            ui_event_tx: None,
            prompt_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn with_ui_event_tx(&self, ui_event_tx: UnboundedSender<ServerMessage>) -> Self {
        *self.agent_event_tx.lock().unwrap() = Some(ui_event_tx.clone());
        Self {
            agent_runtime: Arc::clone(&self.agent_runtime),
            active_executions: Arc::clone(&self.active_executions),
            active_hubs: Arc::clone(&self.active_hubs),
            commit_routers: Arc::clone(&self.commit_routers),
            realtime_routers: Arc::clone(&self.realtime_routers),
            pending_approvals: Arc::clone(&self.pending_approvals),
            pending_interactions: Arc::clone(&self.pending_interactions),
            approval_stores: Arc::clone(&self.approval_stores),
            task_contexts: Arc::clone(&self.task_contexts),
            agent_event_tx: Arc::clone(&self.agent_event_tx),
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

    async fn run_execution_turn_subscription(
        &self,
        input: TurnRunInput,
        persist_sink: Arc<dyn PersistSink>,
        agent_spec: AgentSpec,
    ) -> Result<SessionSubscription, ProtocolError> {
        // Runtime identity is unique per Turn; durable storage is one shard per
        // Execution and binds the shard to its owning AgentInstance.
        let execution_id = format!("exec_{}", uuid::Uuid::new_v4());
        let storage_task_id = execution_id.clone();
        let input_message_id = format!("msg_user_{}", uuid::Uuid::new_v4());
        let hub = Arc::new(orchd::testing::SessionOutputHub::new(
            input.session_id.clone(),
            uuid::Uuid::new_v4().to_string(),
            64,
        ));
        let root_agent_instance_id = format!("agent_{}_root", input.session_id);
        let repository = input.session_dir.as_ref().map(TaskRepository::new);
        let inner_commit: Arc<dyn ExecutionCommitPort> = if let Some(repository) = &repository {
            repository
                .ensure_root_agent(&agent_spec.id)
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
            Arc::new(RepositoryExecutionCommitPort {
                repository: repository.clone(),
            })
        } else {
            let legacy = Arc::new(LegacyPersistExecutionCommitPort::for_root_shard(
                Arc::clone(&persist_sink),
                agent_spec.id.clone(),
                storage_task_id.clone(),
                0,
            ));
            legacy
                .ensure_root_shard_if_needed(&input.session_id, true)
                .await
                .map_err(|err| ProtocolError::InvalidCommand(err.to_string()))?;
            legacy
        };

        let commit: Arc<dyn ExecutionCommitPort> = Arc::new(NotifyingExecutionCommitPort::new(
            inner_commit,
            Arc::clone(&hub),
            agent_spec.id.clone(),
            storage_task_id.clone(),
        ));

        // User message is committed by host before start_execution.
        commit
            .commit_message(piko_protocol::execution::MessageCommit {
                session_id: input.session_id.clone(),
                turn_id: input.turn_id.clone(),
                execution_id: execution_id.clone(),
                agent_instance_id: root_agent_instance_id.clone(),
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

        let fallback = repository.as_ref().map(|repository| {
            Arc::new(RepositoryExecutionCommitPort {
                repository: repository.clone(),
            }) as Arc<dyn ExecutionCommitPort>
        });
        let router = {
            let mut routers = self.commit_routers.lock().unwrap();
            Arc::clone(
                routers
                    .entry(input.session_id.clone())
                    .or_insert_with(|| Arc::new(ExecutionCommitRouter::new(fallback))),
            )
        };
        router.register(execution_id.clone(), Arc::clone(&commit));
        let realtime_router = {
            let mut routers = self.realtime_routers.lock().unwrap();
            Arc::clone(
                routers
                    .entry(input.session_id.clone())
                    .or_insert_with(|| Arc::new(RealtimeDeltaRouter::default())),
            )
        };
        realtime_router.register(execution_id.clone(), Arc::clone(&hub));

        if matches!(
            self.agent_runtime
                .agent_snapshot(input.session_id.clone(), root_agent_instance_id.clone())
                .await,
            Err(orchd_api::AgentApiError::SessionNotAttached)
        ) {
            let (root, agent_commit): (AgentInstanceIdentity, Arc<dyn AgentCommitPort>) =
                if let Some(repository) = &repository {
                    (
                        repository
                            .ensure_root_agent(&agent_spec.id)
                            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?,
                        Arc::new(repository.clone()),
                    )
                } else {
                    (
                        AgentInstanceIdentity {
                            session_id: input.session_id.clone(),
                            agent_instance_id: root_agent_instance_id.clone(),
                            agent_spec_id: agent_spec.id.clone(),
                            parent_agent_instance_id: None,
                        },
                        Arc::new(EphemeralAgentCommitPort::default()),
                    )
                };
            let recovered_agents = if let Some(repository) = &repository {
                let resolved_specs = crate::domain::agents::loader::load_agents(&input.cwd);
                repository
                    .interrupt_incomplete_agent_executions()
                    .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
                repository
                    .agent_instances()
                    .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?
                    .into_iter()
                    .map(|agent| {
                        let agent_instance_id = agent.identity.agent_instance_id.clone();
                        let recovered_spec_id = agent.identity.agent_spec_id.clone();
                        let mut transcript = repository
                            .agent_transcript(&input.session_id, &agent_instance_id)
                            .unwrap_or_default();
                        if agent_instance_id == root.agent_instance_id && transcript.is_empty() {
                            transcript = input
                                .resume_root_task
                                .as_ref()
                                .map(|resume| resume.state.transcript.clone())
                                .unwrap_or_default();
                        }
                        AgentRecoveryState {
                            inbox: repository
                                .agent_inbox(&agent_instance_id)
                                .unwrap_or_default(),
                            identity: agent.identity,
                            spec: agent.spec.unwrap_or_else(|| {
                                resolved_specs
                                    .get(&recovered_spec_id)
                                    .cloned()
                                    .or_else(|| {
                                        resolved_specs
                                            .values()
                                            .find(|spec| spec.id == recovered_spec_id)
                                            .cloned()
                                    })
                                    .unwrap_or_else(|| agent_spec.clone())
                            }),
                            lifecycle: agent.lifecycle,
                            transcript,
                            latest_report: agent.latest_report,
                            execution_reports: repository
                                .agent_execution_reports(&agent_instance_id)
                                .unwrap_or_default(),
                        }
                    })
                    .collect()
            } else {
                vec![AgentRecoveryState {
                    identity: root.clone(),
                    spec: agent_spec.clone(),
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: input
                        .resume_root_task
                        .as_ref()
                        .map(|resume| resume.state.transcript.clone())
                        .unwrap_or_default(),
                    inbox: Vec::new(),
                    latest_report: None,
                    execution_reports: Vec::new(),
                }]
            };
            let agent_commit: Arc<dyn AgentCommitPort> = Arc::new(ProjectingAgentCommitPort::new(
                agent_commit,
                &recovered_agents,
                Arc::clone(&self.agent_event_tx),
            ));
            self.agent_runtime
                .attach_agent_session(SessionAgentConfig {
                    session_id: input.session_id.clone(),
                    root,
                    recovered_agents,
                    ports: SessionAgentPorts {
                        agents: agent_commit,
                        executions: SessionExecutionPorts::new(
                            router.clone() as Arc<dyn ExecutionCommitPort>
                        )
                        .with_realtime(realtime_router as Arc<dyn RealtimeDeltaSink>),
                    },
                })
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        }

        let receipt = self
            .agent_runtime
            .send_agent_input(SendAgentInputRequest {
                request_id: format!("req_{}", uuid::Uuid::new_v4()),
                session_id: input.session_id.clone(),
                agent_instance_id: root_agent_instance_id.clone(),
                caller_agent_instance_id: None,
                requested_execution_id: Some(execution_id.clone()),
                message_id: input_message_id,
                content: MessageContent::String(input.prompt.clone()),
                delivery: AgentInputDelivery::StartWhenIdle,
            })
            .await
            .map_err(|err| ProtocolError::InvalidCommand(err.to_string()))?;

        tracing::info!(
            session_id = %input.session_id,
            turn_id = %input.turn_id,
            execution_id = %receipt.execution_id.as_deref().unwrap_or("unknown"),
            storage_task_id = %storage_task_id,
            "execution runtime path started"
        );

        {
            let mut active = self.active_executions.lock().unwrap();
            active.insert(
                input.session_id.clone(),
                (input.turn_id.clone(), execution_id.clone()),
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
                agent_instance_id: root_agent_instance_id.clone(),
                task_id: execution_id.clone(),
                agent_id: agent_spec.id.clone(),
                task_seq: 0,
                cursor: hub.cursor(),
                event: piko_protocol::agent_runtime::SessionEvent::ExecutionChanged {
                    snapshot: piko_protocol::ExecutionObservationSnapshot {
                        session_id: input.session_id.clone(),
                        turn_id: input.turn_id.clone(),
                        execution_id: execution_id.clone(),
                        agent_instance_id: root_agent_instance_id.clone(),
                        agent_id: agent_spec.id.clone(),
                        status: piko_protocol::ExecutionStatus::Running,
                    },
                },
            })
            .await;

        let agent_runtime = Arc::clone(&self.agent_runtime);
        let active_executions = Arc::clone(&self.active_executions);
        let active_hubs = Arc::clone(&self.active_hubs);
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        let agent_id = agent_spec.id.clone();
        let agent_instance_id = root_agent_instance_id;
        let hub_for_terminal = Arc::clone(&hub);
        tokio::spawn(async move {
            let outcome = agent_runtime
                .wait_agent_execution(
                    session_id.clone(),
                    agent_instance_id.clone(),
                    execution_id.clone(),
                )
                .await
                .map(|report| report.outcome);
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
                    agent_instance_id: agent_instance_id.clone(),
                    task_id: execution_id.clone(),
                    agent_id: agent_id.clone(),
                    task_seq: 0,
                    cursor: hub_for_terminal.cursor(),
                    event: piko_protocol::agent_runtime::SessionEvent::ExecutionChanged {
                        snapshot: piko_protocol::ExecutionObservationSnapshot {
                            session_id: session_id.clone(),
                            turn_id,
                            execution_id: execution_id.clone(),
                            agent_instance_id,
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
    if !spec.tool_set_ids.iter().any(|id| id == "multi_agent") {
        spec.tool_set_ids.push("multi_agent".into());
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

        self.agent_runtime
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
        self.agent_runtime.register_agent(agent_spec.clone()).await;
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
        let Some(_execution_id) = execution_id else {
            return false;
        };
        self.agent_runtime
            .steer_agent(piko_protocol::SteerAgentRequest {
                request_id: format!("req_steer_{}", uuid::Uuid::new_v4()),
                session_id: session_id.to_string(),
                agent_instance_id: format!("agent_{session_id}_root"),
                caller_agent_instance_id: None,
                message_id: format!("msg_steer_{}", uuid::Uuid::new_v4()),
                content: MessageContent::String(message.to_string()),
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
        self.agent_runtime
            .request_cancel_execution(CancelExecutionRequest {
                request_id: format!("req_cancel_{}", uuid::Uuid::new_v4()),
                session_id: session_id.to_string(),
                execution_id,
                reason: CancelReason::UserRequested,
            })
            .await
            .map(|receipt| receipt.accepted)
            .unwrap_or(false)
    }

    async fn list_agent_instances(&self, session_id: &str) -> Option<Vec<crate::api::AgentInfo>> {
        let snapshots = self
            .agent_runtime
            .list_agents(session_id.to_string())
            .await
            .ok()?;
        Some(
            snapshots
                .into_iter()
                .map(|snapshot| {
                    let status = match (&snapshot.lifecycle, &snapshot.activity) {
                        (AgentInstanceLifecycle::Closed, _) => crate::api::AgentStatus::Closed,
                        (AgentInstanceLifecycle::Terminated, _) => crate::api::AgentStatus::Stopped,
                        (AgentInstanceLifecycle::Unavailable, _) => crate::api::AgentStatus::Failed,
                        (_, piko_protocol::AgentActivity::Running { .. })
                        | (_, piko_protocol::AgentActivity::WaitingForApproval { .. })
                        | (_, piko_protocol::AgentActivity::Cancelling { .. }) => {
                            crate::api::AgentStatus::Running
                        }
                        _ => crate::api::AgentStatus::Idle,
                    };
                    crate::api::AgentInfo {
                        agent_instance_id: snapshot.identity.agent_instance_id.clone(),
                        agent_id: snapshot.identity.agent_spec_id.clone(),
                        parent_agent_instance_id: snapshot
                            .identity
                            .parent_agent_instance_id
                            .clone(),
                        lifecycle: snapshot.lifecycle,
                        activity: snapshot.activity,
                        unread_report_count: snapshot.unread_report_count,
                        task_id: snapshot.identity.agent_instance_id,
                        parent_task_id: snapshot.identity.parent_agent_instance_id,
                        name: snapshot.identity.agent_spec_id,
                        role: "assistant".into(),
                        status,
                    }
                })
                .collect(),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    struct FailingAgentCommitPort;

    #[async_trait]
    impl AgentCommitPort for FailingAgentCommitPort {
        async fn commit_agent_command(
            &self,
            _session_id: &str,
            _command: AgentDurableCommand,
        ) -> Result<AgentCommitAck, CommitError> {
            Err(CommitError::Unavailable)
        }
    }

    fn create_command() -> AgentDurableCommand {
        AgentDurableCommand::Create {
            identity: AgentInstanceIdentity {
                session_id: "session".into(),
                agent_instance_id: "child".into(),
                agent_spec_id: "worker".into(),
                parent_agent_instance_id: Some("root".into()),
            },
            spec: AgentSpec {
                id: "worker".into(),
                name: "Worker".into(),
                role: "worker".into(),
                description: None,
                system_prompt: "work".into(),
                model: None,
                thinking_level: None,
                tool_set_ids: Vec::new(),
                active_tool_names: None,
            },
        }
    }

    #[tokio::test]
    async fn agent_projection_is_emitted_only_after_durable_ack() {
        let (event_tx, mut event_rx) = unbounded_channel();
        let event_tx = Arc::new(std::sync::Mutex::new(Some(event_tx)));
        let committing = ProjectingAgentCommitPort::new(
            Arc::new(EphemeralAgentCommitPort::default()),
            &[],
            Arc::clone(&event_tx),
        );
        committing
            .commit_agent_command("session", create_command())
            .await
            .unwrap();
        assert!(matches!(
            event_rx.try_recv(),
            Ok(ServerMessage::AgentChanged(info)) if info.agent_instance_id == "child"
        ));

        let failing = ProjectingAgentCommitPort::new(
            Arc::new(FailingAgentCommitPort),
            &[],
            Arc::clone(&event_tx),
        );
        assert!(
            failing
                .commit_agent_command("session", create_command())
                .await
                .is_err()
        );
        assert!(event_rx.try_recv().is_err());
    }
}
