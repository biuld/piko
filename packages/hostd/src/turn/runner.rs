use std::sync::Arc;

use async_trait::async_trait;

use crate::api::{Event, ProtocolError};
use llmd::gateway::LlmGateway;
use orchd::orchestrator::core::OrchCore;
use orchd::protocol::agents::AgentSpec;
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions, OrchRunResult};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone)]
pub struct TurnRunInput {
    pub session_id: String,
    pub turn_id: String,
    pub prompt: String,
    pub system_prompt: String,
    /// Active tool names to enable. None = all tools enabled.
    pub active_tool_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct TurnRunOutput {
    pub events: Vec<Event>,
    pub total_tasks: u32,
}

#[async_trait]
pub trait TurnRunner: Send + Sync {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, ProtocolError>;

    async fn respond_approval(
        &self,
        _approval_id: &str,
        _decision: crate::api::ApprovalDecision,
    ) -> Result<bool, ProtocolError> {
        Ok(false)
    }

    /// Route a steering message to the active orchd task.
    /// Returns true if the steering was delivered.
    async fn steer_task(
        &self,
        _task_id: &str,
        _source_task_id: &str,
        _source_agent_id: &str,
        _message: &str,
    ) -> bool {
        false
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockTurnRunner;

#[async_trait]
impl TurnRunner for MockTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, ProtocolError> {
        let _ = input;
        Ok(TurnRunOutput {
            events: Vec::new(),
            total_tasks: 1,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ErrorTurnRunner {
    message: String,
}

impl ErrorTurnRunner {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[async_trait]
impl TurnRunner for ErrorTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Result<TurnRunOutput, ProtocolError> {
        let _ = input;
        Err(ProtocolError::InvalidCommand(self.message.clone()))
    }
}

#[derive(Clone)]
pub struct OrchTurnRunner {
    core: Arc<OrchCore>,
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

    pub async fn new_with_mcp(
        model_executor: Arc<dyn LlmGateway>,
        provider: &str,
        api_key: &str,
        model_id: &str,
        thinking_level: Option<piko_protocol::model::ThinkingLevel>,
        thinking_level_map: piko_protocol::model::ThinkingLevelMap,
        mcp_configs: &[crate::mcp::McpServerConfig],
        sandbox_settings: Option<&crate::settings::SandboxSettings>,
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
        let core = OrchCore::from_config(model_executor, config).await;

        // Initialize MCP tools
        let registry = core.tool_registry.clone();
        let registered = crate::mcp::initialize_mcp_tools(mcp_configs, registry).await;
        if !registered.is_empty() {
            tracing::info!("MCP tools registered: {:?}", registered);
        }

        Self { core }
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
        let agent_id = format!("hostd_{turn_id}");

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
        self.core.register_agent(agent_spec.clone()).await;

        // Subscribe to host-facing events from orchd.
        let (host_tx, mut host_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
        let cleanup = self
            .core
            .subscribe_host_events(
                session_id.clone(),
                agent_id.clone(),
                Box::new(move |event| {
                    let _ = host_tx.send(event);
                }),
            )
            .await;

        // Spawn root task in background
        let core = self.core.clone();
        let prompt = input.prompt.clone();
        let run_agent_id = agent_id.clone();
        let run_session_id = session_id.clone();
        let run_turn_id = turn_id.clone();
        let run = tokio::spawn(async move {
            core.run(
                &prompt,
                Some(OrchRunOptions {
                    command: OrchRunCommandOptions {
                        target_agent_id: Some(run_agent_id),
                    },
                    history: None,
                    host_context: Some(orchd::protocol::agents::HostTaskContext {
                        session_id: run_session_id,
                        turn_id: run_turn_id,
                    }),
                }),
            )
            .await
        });
        tokio::pin!(run);

        // Track all tasks in this turn
        let mut pending_tasks: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut total_task_count: u32 = 0;
        let mut root_done = false;
        let mut run_result: Option<OrchRunResult> = None;
        let mut run_joined = false;

        loop {
            if root_done && run_joined && run_result.is_some() && pending_tasks.is_empty() {
                while let Ok(event) = host_rx.try_recv() {
                    emit_or_collect(&mut events, event, &event_tx);
                }
                break;
            }

            if root_done && run_joined && pending_tasks.is_empty() {
                break;
            }

            tokio::select! {
                event = host_rx.recv() => {
                    let Some(event) = event else {
                        if root_done && run_joined {
                            break;
                        }
                        continue;
                    };

                    match &event {
                        Event::TaskCreated { task_id, .. } => {
                            pending_tasks.insert(task_id.clone());
                            total_task_count += 1;
                        }
                        Event::TaskCompleted { task_id, .. }
                        | Event::TaskFailed { task_id, .. }
                        | Event::TaskCancelled { task_id, .. } => {
                            pending_tasks.remove(task_id);
                        }
                        _ => {}
                    }

                    emit_or_collect(&mut events, event, &event_tx);
                }
                result = &mut run, if !run_joined => {
                    run_joined = true;
                    match result {
                        Ok(r) => {
                            run_result = Some(r);
                        }
                        Err(error) => {
                            return Err(ProtocolError::InvalidCommand(
                                format!("orchd run join failed: {error}")
                            ));
                        }
                    }
                    root_done = true;
                }
            }
        }

        cleanup();
        self.core.unregister_agent(&agent_id).await;

        Ok(TurnRunOutput {
            events,
            total_tasks: total_task_count.max(1),
        })
    }

    async fn steer_task(
        &self,
        task_id: &str,
        source_task_id: &str,
        source_agent_id: &str,
        message: &str,
    ) -> bool {
        self.core
            .steer_task(task_id, source_task_id, source_agent_id, message)
            .await
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
