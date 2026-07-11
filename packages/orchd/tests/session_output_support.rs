//! Test helpers: drain SessionSubscription into legacy ServerMessage events.

use std::collections::HashMap;
use std::pin::Pin;

use futures_core::Stream;
use orchd::SessionSubscription;
use piko_protocol::agent_runtime::{RealtimeDelta, SessionEvent, SessionOutput, TaskStatus};
use piko_protocol::{ContentBlock, DisplayEvent, Message, ServerMessage as Event, TaskEvent};
use tokio_stream::StreamExt;

pub fn subscription_event_stream(
    subscription: SessionSubscription,
) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    let session_id = subscription.session_id.clone();
    let mut output = subscription.output;
    Box::pin(async_stream::stream! {
        let mut text_by_message: HashMap<String, String> = HashMap::new();

        while let Some(item) = output.next().await {
            let Ok(envelope) = item else { continue };
            match envelope.output {
                SessionOutput::Delta(delta_envelope) => {
                    let task_id = delta_envelope.task_id.clone();
                    let agent_id = delta_envelope.agent_id.clone();
                    let message_id = delta_envelope.message_id.clone();
                    match delta_envelope.delta {
                        RealtimeDelta::Text { delta, content_index } => {
                            if let Some(message_id) = message_id.clone() {
                                text_by_message
                                    .entry(message_id.clone())
                                    .or_default()
                                    .push_str(&delta);
                                yield Event::Display(DisplayEvent::TextDelta {
                                    message_id,
                                    task_id,
                                    agent_id,
                                    content_index,
                                    delta,
                                });
                            }
                        }
                        RealtimeDelta::Thinking { delta, content_index } => {
                            if let Some(message_id) = message_id.clone() {
                                yield Event::Display(DisplayEvent::ThinkingDelta {
                                    message_id,
                                    task_id,
                                    agent_id,
                                    content_index,
                                    delta,
                                });
                            }
                        }
                        RealtimeDelta::ToolCall {
                            delta,
                            content_index,
                            tool_call_id,
                        } => {
                            if let Some(message_id) = message_id.clone() {
                                yield Event::Display(DisplayEvent::ToolCallDelta {
                                    message_id,
                                    task_id,
                                    agent_id,
                                    content_index,
                                    tool_call_id,
                                    delta,
                                });
                            }
                        }
                        RealtimeDelta::MessageStarted { role } => {
                            if let Some(message_id) = message_id.clone() {
                                yield Event::Display(DisplayEvent::MessageStart {
                                    message_id,
                                    task_id,
                                    agent_id,
                                    role,
                                });
                            }
                        }
                        RealtimeDelta::MessageEnded {
                            stop_reason,
                            error_message,
                        } => {
                            if let Some(message_id) = message_id.clone() {
                                yield Event::Display(DisplayEvent::MessageEnd {
                                    message_id: message_id.clone(),
                                    task_id: task_id.clone(),
                                    agent_id: agent_id.clone(),
                                    stop_reason: stop_reason.clone(),
                                    error_message: error_message.clone(),
                                });
                                if let Some(text) = text_by_message.remove(&message_id)
                                    && !text.is_empty()
                                {
                                    yield Event::Display(DisplayEvent::Finalized {
                                        message_id,
                                        task_id,
                                        agent_id,
                                        content: vec![ContentBlock::Text { text }],
                                        usage: None,
                                        stop_reason,
                                        error_message,
                                    });
                                }
                            }
                        }
                    }
                }
                SessionOutput::Event(event_envelope) => {
                    let task_id = event_envelope.task_id.clone();
                    let agent_id = event_envelope.agent_id.clone();
                    match event_envelope.event {
                        SessionEvent::TaskChanged { snapshot } => {
                            if let Some(task_event) = task_event_from_snapshot(&snapshot) {
                                yield Event::TaskLifecycle(task_event);
                            }
                        }
                        SessionEvent::MessageCommitted {
                            message_id,
                            work_id,
                            role,
                        } => {
                            let message = match role {
                                piko_protocol::MessageRole::User => Message::User {
                                    content: piko_protocol::MessageContent::String(String::new()),
                                    timestamp: None,
                                },
                                piko_protocol::MessageRole::Assistant => Message::Assistant {
                                    content: Vec::new(),
                                    api: String::new(),
                                    provider: String::new(),
                                    model: String::new(),
                                    usage: None,
                                    stop_reason: None,
                                    error_message: None,
                                    timestamp: None,
                                },
                                _ => Message::User {
                                    content: piko_protocol::MessageContent::String(String::new()),
                                    timestamp: None,
                                },
                            };
                            yield Event::Persist(piko_protocol::PersistEvent::UserCommitted {
                                session_id: session_id.clone(),
                                message_id,
                                task_id,
                                agent_id,
                                work_id,
                                message,
                            });
                        }
                        SessionEvent::ToolCommitted {
                            message_id,
                            work_id,
                            tool_call_id: _,
                        } => {
                            yield Event::Persist(piko_protocol::PersistEvent::ToolResultCommitted {
                                session_id: session_id.clone(),
                                message_id,
                                task_id,
                                agent_id,
                                work_id,
                                message: Message::ToolResult {
                                    tool_call_id: String::new(),
                                    tool_name: None,
                                    content: Vec::new(),
                                    details: None,
                                    is_error: None,
                                    timestamp: None,
                                },
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    })
}

fn task_event_from_snapshot(
    snapshot: &piko_protocol::agent_runtime::TaskSnapshot,
) -> Option<TaskEvent> {
    let session_id = snapshot.session_id.clone();
    let task_id = snapshot.task_id.clone();
    let agent_id = snapshot.agent_id.clone();
    match snapshot.status {
        TaskStatus::Created => Some(TaskEvent::Created {
            session_id,
            turn_id: String::new(),
            task_id,
            agent_id,
            parent_task_id: snapshot.parent_task_id.clone(),
            source_agent_id: None,
            prompt: String::new(),
            timestamp: 0,
        }),
        TaskStatus::Running => Some(TaskEvent::Started {
            session_id,
            task_id,
            agent_id,
            timestamp: 0,
        }),
        TaskStatus::Idle => Some(TaskEvent::Idle {
            session_id,
            task_id,
            agent_id,
            total_steps: 1,
            summary: String::new(),
            timestamp: 0,
        }),
        TaskStatus::Terminated => Some(TaskEvent::Completed {
            session_id,
            task_id,
            agent_id,
            total_steps: 1,
            summary: String::new(),
            final_status: "completed".into(),
            timestamp: 0,
        }),
        TaskStatus::Failed => Some(TaskEvent::Failed {
            session_id,
            task_id,
            agent_id,
            error: "failed".into(),
            timestamp: 0,
        }),
        TaskStatus::Closed => Some(TaskEvent::Closed {
            session_id,
            task_id,
            agent_id,
            timestamp: 0,
        }),
    }
}

/// Collect events from a session subscription stream with a bounded wait.
pub async fn collect_test_events(
    mut stream: std::pin::Pin<Box<dyn Stream<Item = Event> + Send>>,
    timeout: std::time::Duration,
) -> Vec<Event> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut events = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(event)) => events.push(event),
            _ => break,
        }
    }
    events
}
