use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::api::{HostEvent, HostProtocolError};
use orchd::model::executor::ModelStepExecutor;
use orchd::orchestrator::core::OrchCore;
use orchd::protocol::agents::AgentSpec;
use orchd::protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};
use tokio::sync::mpsc::UnboundedSender;

use crate::state::HostState;

#[derive(Debug, Clone)]
pub struct TurnRunInput {
    pub session_id: String,
    pub turn_id: String,
    pub prompt: String,
    pub system_prompt: String,
}

#[derive(Debug, Clone, Default)]
pub struct TurnRunOutput {
    pub events: Vec<HostEvent>,
}

pub trait TurnRunner: Send + Sync {
    fn run_turn<'a>(
        &'a self,
        input: TurnRunInput,
        _state: &'a mut HostState,
        event_tx: Option<UnboundedSender<HostEvent>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, HostProtocolError>> + Send + 'a>>;

    fn respond_approval<'a>(
        &'a self,
        _approval_id: &'a str,
        _decision: crate::api::ApprovalDecision,
    ) -> Pin<Box<dyn Future<Output = Result<bool, HostProtocolError>> + Send + 'a>> {
        Box::pin(async { Ok(false) })
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockTurnRunner;

impl TurnRunner for MockTurnRunner {
    fn run_turn<'a>(
        &'a self,
        input: TurnRunInput,
        state: &'a mut HostState,
        _event_tx: Option<UnboundedSender<HostEvent>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, HostProtocolError>> + Send + 'a>> {
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
        _event_tx: Option<UnboundedSender<HostEvent>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, HostProtocolError>> + Send + 'a>> {
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
    pub async fn new(model_executor: Arc<dyn ModelStepExecutor>) -> Self {
        use orchd::protocol::config::OrchdConfig;
        let config = OrchdConfig::single_provider(
            "anthropic".to_string(),
            String::new(),
            "claude-sonnet-4-20250514".to_string(),
        );
        let core = OrchCore::from_config(model_executor, config).await;
        Self { core }
    }
}

impl TurnRunner for OrchTurnRunner {
    fn run_turn<'a>(
        &'a self,
        input: TurnRunInput,
        _state: &'a mut HostState,
        event_tx: Option<UnboundedSender<HostEvent>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, HostProtocolError>> + Send + 'a>> {
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
                active_tool_names: None,
            };
            self.core.register_agent(agent_spec.clone()).await;

            // Subscribe to host-facing events from orchd.
            let (host_tx, mut host_rx) = tokio::sync::mpsc::unbounded_channel::<HostEvent>();
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

            // Run the task
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
            let start_ev = HostEvent::TurnStarted {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                root_task_id: agent_id.clone(),
                timestamp: now_ms(),
            };
            emit_or_collect(&mut events, start_ev, &event_tx);

            let _result = loop {
                tokio::select! {
                    event = host_rx.recv() => {
                        if let Some(event) = event {
                            emit_or_collect(&mut events, event, &event_tx);
                        }
                    }
                    result = &mut run => {
                        break result
                            .map_err(|error| HostProtocolError::InvalidCommand(format!("orchd run join failed: {error}")))?;
                    }
                }
            };

            // Drain remaining events
            while let Ok(event) = host_rx.try_recv() {
                emit_or_collect(&mut events, event, &event_tx);
            }

            cleanup();
            self.core.unregister_agent(&agent_id).await;

            // Emit turn completed
            let complete_ev = HostEvent::TurnCompleted {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                total_tasks: 1,
                timestamp: now_ms(),
            };
            emit_or_collect(&mut events, complete_ev, &event_tx);

            Ok(TurnRunOutput { events })
        })
    }
}

fn emit_or_collect(
    events: &mut Vec<HostEvent>,
    event: HostEvent,
    event_tx: &Option<UnboundedSender<HostEvent>>,
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
