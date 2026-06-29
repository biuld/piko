use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio_stream::{StreamExt, StreamMap};

use llmd::gateway::LlmGateway;
use orchd::AgentReport;
use orchd::Supervisor;
use orchd::application::PendingStream;
use orchd::domain::tools::approval::{ToolApprovalDecision, ToolApprovalRequest};
use orchd::ports::ApprovalGateway;
use orchd::protocol::agents::{AgentSpec, HostTaskContext};
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

use crate::api::{Event, ProtocolError};
use crate::domain::config::{McpServerConfig, SandboxSettings};
use crate::domain::turns::approval::{ApprovalScope, ApprovalStore};
use crate::domain::turns::runner::{TurnRunInput, TurnRunOutput, TurnRunner};

#[derive(Clone)]
pub struct OrchTurnRunner {
    supervisor: Arc<Supervisor>,
    pending_approvals:
        Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<crate::api::ApprovalDecision>>>>,
    approval_stores: Arc<std::sync::Mutex<HashMap<String, Arc<ApprovalStore>>>>,
    task_contexts: Arc<std::sync::Mutex<HashMap<String, (String, String)>>>, // task_id -> (session_id, cwd)
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
            approval_stores: Arc::new(std::sync::Mutex::new(HashMap::new())),
            task_contexts: Arc::new(std::sync::Mutex::new(HashMap::new())),
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
}

#[async_trait]
impl TurnRunner for OrchTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, ProtocolError> {
        let mut events = Vec::new();
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        let agent_id = "main".to_string();

        // Register approval gateway on tool registry
        let registry = self.supervisor.tool_registry().clone();
        registry
            .set_approval_gateway(Some(Box::new(self.clone())))
            .await;

        // Register agent
        let agent_spec = AgentSpec {
            id: agent_id.clone(),
            name: agent_id.clone(),
            role: "assistant".into(),
            description: Some("hostd-managed agent".into()),
            system_prompt: input.system_prompt.clone(),
            model: None,
            tool_set_ids: vec!["builtin".into(), "workspace".into()],
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

        // StreamMap with the root stream as initial entry
        let mut map: StreamMap<String, Pin<Box<dyn Stream<Item = Event> + Send>>> =
            StreamMap::new();
        map.insert("root".into(), root_stream);

        let mut total_task_count: u32 = 0;
        let mut next_sub_idx: u32 = 0;

        // Consume from StreamMap — polls all streams concurrently
        while let Some((_stream_key, event)) = map.next().await {
            // Register task context
            if let Event::TaskCreated {
                task_id,
                session_id,
                ..
            } = &event
            {
                self.register_task_context(task_id.clone(), session_id.clone(), input.cwd.clone());
            }

            if matches!(&event, Event::TaskCreated { .. }) {
                total_task_count += 1;
            }

            // Record sub-agent results for spawn / await_task
            match &event {
                Event::TaskCompleted {
                    task_id,
                    summary,
                    final_status,
                    total_steps,
                    ..
                } => {
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
                Event::TaskFailed { task_id, error, .. } => {
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
                Event::TaskCancelled { task_id, .. } => {
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

            emit_or_collect(&mut events, event, &event_tx);

            // After each event, poll for newly spawned sub-agent streams
            let new_streams: Vec<PendingStream> = self.supervisor.take_pending_streams().await;
            for ps in new_streams {
                let key = format!("sub_{}", next_sub_idx);
                next_sub_idx += 1;
                map.insert(key, ps.stream);
            }
        }

        self.supervisor.unregister_agent(&agent_id).await;

        Ok(TurnRunOutput {
            events,
            total_tasks: total_task_count.max(1),
        })
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
}

#[async_trait]
impl ApprovalGateway for OrchTurnRunner {
    async fn request_tool_approval(&self, request: ToolApprovalRequest) -> ToolApprovalDecision {
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

        // 6. Map and return
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

fn emit_or_collect(
    events: &mut Vec<Event>,
    event: Event,
    event_tx: &Option<UnboundedSender<Event>>,
) {
    if let Some(tx) = event_tx {
        let _ = tx.send(event);
    } else {
        events.push(event);
    }
}
