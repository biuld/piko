use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::api::{Event, ProtocolError};
use piko_protocol::executor::LlmGateway;
use orchd::orchestrator::core::OrchCore;
use orchd::protocol::agents::AgentSpec;
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions, OrchRunResult};
use tokio::sync::mpsc::UnboundedSender;

use crate::state::HostState;

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
}

pub trait TurnRunner: Send + Sync {
    fn run_turn<'a>(
        &'a self,
        input: TurnRunInput,
        _state: &'a mut HostState,
        event_tx: Option<UnboundedSender<Event>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, ProtocolError>> + Send + 'a>>;

    fn respond_approval<'a>(
        &'a self,
        _approval_id: &'a str,
        _decision: crate::api::ApprovalDecision,
    ) -> Pin<Box<dyn Future<Output = Result<bool, ProtocolError>> + Send + 'a>> {
        Box::pin(async { Ok(false) })
    }

    /// Route a steering message to the active orchd task.
    /// Returns true if the steering was delivered.
    fn steer_task<'a>(
        &'a self,
        _task_id: &'a str,
        _source_task_id: &'a str,
        _source_agent_id: &'a str,
        _message: &'a str,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockTurnRunner;

impl TurnRunner for MockTurnRunner {
    fn run_turn<'a>(
        &'a self,
        input: TurnRunInput,
        state: &'a mut HostState,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, ProtocolError>> + Send + 'a>> {
        Box::pin(async move {
            let (_turn_id, start_events) = state.start_turn(&input.session_id)?;
            let complete_ev = state.complete_turn(&input.session_id, &input.turn_id)?;
            let mut events = start_events;
            events.push(complete_ev);
            Ok(TurnRunOutput { events })
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

impl TurnRunner for ErrorTurnRunner {
    fn run_turn<'a>(
        &'a self,
        input: TurnRunInput,
        state: &'a mut HostState,
        _event_tx: Option<UnboundedSender<Event>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, ProtocolError>> + Send + 'a>> {
        Box::pin(async move {
            let fail_ev =
                state.fail_turn(&input.session_id, &input.turn_id, self.message.clone())?;
            Ok(TurnRunOutput {
                events: vec![fail_ev],
            })
        })
    }
}

#[derive(Clone)]
pub struct OrchTurnRunner {
    core: Arc<OrchCore>,
}

impl OrchTurnRunner {
    pub async fn new(model_executor: Arc<dyn LlmGateway>) -> Self {
        Self::new_with_mcp(model_executor, &[]).await
    }

    pub async fn new_with_mcp(
        model_executor: Arc<dyn LlmGateway>,
        mcp_configs: &[crate::mcp::McpServerConfig],
    ) -> Self {
        use orchd::protocol::config::OrchdConfig;
        let config = OrchdConfig::single_provider(
            "anthropic".to_string(),
            String::new(),
            "claude-sonnet-4-20250514".to_string(),
        );
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

impl TurnRunner for OrchTurnRunner {
    fn run_turn<'a>(
        &'a self,
        input: TurnRunInput,
        _state: &'a mut HostState,
        event_tx: Option<UnboundedSender<Event>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, ProtocolError>> + Send + 'a>> {
        Box::pin(async move {
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

            // Emit turn started
            let start_ev = Event::TurnStarted {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                root_task_id: agent_id.clone(),
                timestamp: now_ms(),
            };
            emit_or_collect(&mut events, start_ev, &event_tx);

            // Track all tasks in this turn: pending set of task_ids not yet terminal.
            // Also track the total count for turn_completed.
            let mut pending_tasks: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut total_task_count: u32 = 0;
            let mut root_done = false;
            let mut run_result: Option<OrchRunResult> = None;
            let mut run_joined = false;

            loop {
                // If root is done and run has been joined, and no pending tasks remain, break.
                if root_done && run_joined && run_result.is_some() && pending_tasks.is_empty() {
                    // Drain any final flush events
                    while let Ok(event) = host_rx.try_recv() {
                        emit_or_collect(&mut events, event, &event_tx);
                    }
                    break;
                }

                // If we're waiting for nothing and run is gone, bail out.
                if root_done && run_joined && pending_tasks.is_empty() {
                    break;
                }

                tokio::select! {
                    event = host_rx.recv() => {
                        let Some(event) = event else {
                            // Channel closed — all events received
                            if root_done && run_joined {
                                break;
                            }
                            continue;
                        };

                        // Track task lifecycle from events
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
                        // Give one more tick for pending task events
                    }
                }
            }

            cleanup();
            self.core.unregister_agent(&agent_id).await;

            // Emit turn completed with actual task count
            let complete_ev = Event::TurnCompleted {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                total_tasks: total_task_count.max(1),
                timestamp: now_ms(),
            };
            emit_or_collect(&mut events, complete_ev, &event_tx);

            Ok(TurnRunOutput { events })
        })
    }

    fn steer_task<'a>(
        &'a self,
        task_id: &'a str,
        source_task_id: &'a str,
        source_agent_id: &'a str,
        message: &'a str,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let core = self.core.clone();
        let task_id = task_id.to_string();
        let source_task_id = source_task_id.to_string();
        let source_agent_id = source_agent_id.to_string();
        let message = message.to_string();
        Box::pin(async move {
            core.steer_task(&task_id, &source_task_id, &source_agent_id, &message)
                .await
        })
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

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
