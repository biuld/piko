use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::api::{HostEvent, HostProtocolError};
use orchd::model::executor::ModelStepExecutor;
use orchd::orchestrator::core::OrchCore;
use orchd::protocol::agents::AgentSpec;
use orchd::protocol::events::OrchEvent;
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
        state: &'a mut HostState,
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
            let fail_ev = state.fail_turn(&input.session_id, &input.turn_id, self.message.clone())?;
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
        state: &'a mut HostState,
        event_tx: Option<UnboundedSender<HostEvent>>,
    ) -> Pin<Box<dyn Future<Output = Result<TurnRunOutput, HostProtocolError>> + Send + 'a>> {
        Box::pin(async move {
            let mut events = Vec::new();
            let session_id = input.session_id.clone();
            let turn_id = input.turn_id.clone();
            let agent_id = format!("hostd_{turn_id}");

            // Register agent
            let mut agent_spec = AgentSpec {
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

            // Subscribe to orchestrator events
            let (orch_tx, mut orch_rx) = tokio::sync::mpsc::unbounded_channel::<OrchEvent>();
            let cleanup = self
                .core
                .subscribe_orch(Box::new(move |event| {
                    let _ = orch_tx.send(event);
                }))
                .await;

            // Run the task
            let core = self.core.clone();
            let prompt = input.prompt.clone();
            let run_agent_id = agent_id.clone();
            let run = tokio::spawn(async move {
                core.run(
                    &prompt,
                    Some(OrchRunOptions {
                        command: OrchRunCommandOptions {
                            target_agent_id: Some(run_agent_id),
                        },
                        history: None,
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

            let result = loop {
                tokio::select! {
                    event = orch_rx.recv() => {
                        if let Some(event) = event {
                            if let Some(host_event) = map_orch_to_host_event(&session_id, &turn_id, &agent_id, event) {
                                emit_or_collect(&mut events, host_event, &event_tx);
                            }
                        }
                    }
                    result = &mut run => {
                        break result
                            .map_err(|error| HostProtocolError::InvalidCommand(format!("orchd run join failed: {error}")))?;
                    }
                }
            };

            // Drain remaining events
            while let Ok(event) = orch_rx.try_recv() {
                if let Some(host_event) = map_orch_to_host_event(&session_id, &turn_id, &agent_id, event) {
                    emit_or_collect(&mut events, host_event, &event_tx);
                }
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

fn map_orch_to_host_event(
    session_id: &str,
    _turn_id: &str,
    agent_id: &str,
    event: OrchEvent,
) -> Option<HostEvent> {
    let ts = now_ms();
    match event {
        OrchEvent::TextDelta { message_id, delta } => Some(HostEvent::TextDelta {
            task_id: agent_id.to_string(),
            agent_id: agent_id.to_string(),
            message_id,
            delta,
        }),
        OrchEvent::ThinkingDelta { message_id, delta } => Some(HostEvent::ThinkingDelta {
            task_id: agent_id.to_string(),
            agent_id: agent_id.to_string(),
            message_id,
            delta,
        }),
        OrchEvent::MessageStart { message_id, agent_id: ev_agent, task_id } => Some(HostEvent::MessageStart {
            task_id,
            agent_id: ev_agent,
            message_id,
            role: crate::api::MessageRole::Assistant,
        }),
        OrchEvent::MessageEnd { message_id, stop_reason } => Some(HostEvent::MessageEnd {
            task_id: agent_id.to_string(),
            agent_id: agent_id.to_string(),
            message_id,
            stop_reason: Some(stop_reason),
        }),
        OrchEvent::ToolStart { tool_call_id, tool_name, agent_id: ev_agent, task_id } => Some(HostEvent::ToolStart {
            task_id,
            agent_id: ev_agent,
            tool_call_id,
            tool_name,
            args: serde_json::Value::Null,
            parent_message_id: None,
        }),
        OrchEvent::ToolEnd { tool_call_id, ok, output } => Some(HostEvent::ToolEnd {
            task_id: agent_id.to_string(),
            agent_id: agent_id.to_string(),
            tool_call_id,
            tool_name: String::new(),
            result: output,
            is_error: !ok,
        }),
        OrchEvent::RequestApproval { approval_id, action, details, agent_id: ev_agent, task_id } => Some(HostEvent::ApprovalRequested {
            task_id,
            agent_id: ev_agent,
            approval_id,
            tool_name: action,
            tool_args: serde_json::json!({ "details": details }),
        }),
        OrchEvent::TaskError { task_id, error } => Some(HostEvent::TaskFailed {
            session_id: session_id.to_string(),
            task_id,
            agent_id: agent_id.to_string(),
            error,
            timestamp: ts,
        }),
        OrchEvent::TaskEnd { task_id, status, .. } => {
            match status {
                orchd::protocol::events::TaskEndStatus::Completed => Some(HostEvent::TaskCompleted {
                    session_id: session_id.to_string(),
                    task_id,
                    agent_id: agent_id.to_string(),
                    total_steps: 0,
                    summary: String::new(),
                    final_status: "completed".into(),
                    timestamp: ts,
                }),
                orchd::protocol::events::TaskEndStatus::Aborted => Some(HostEvent::TaskCancelled {
                    session_id: session_id.to_string(),
                    task_id,
                    agent_id: agent_id.to_string(),
                    timestamp: ts,
                }),
                orchd::protocol::events::TaskEndStatus::Error => Some(HostEvent::TaskFailed {
                    session_id: session_id.to_string(),
                    task_id,
                    agent_id: agent_id.to_string(),
                    error: "orchd task error".into(),
                    timestamp: ts,
                }),
            }
        }
        OrchEvent::AskUser { .. }
        | OrchEvent::SubAgentSpawned { .. }
        | OrchEvent::SubAgentCompleted { .. }
        | OrchEvent::PlanUpdated { .. } => None,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
