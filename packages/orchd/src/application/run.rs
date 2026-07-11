// ---- Run — synchronous convenience run for tests and tooling ----

use std::collections::HashMap;

use futures_util::StreamExt;
use piko_protocol::agent_runtime::{SessionOutput, TaskStatus};
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};
use piko_protocol::{ContentBlock, Message};

use crate::application::service::AgentRuntimeService;

use super::supervision::Supervisor;
use super::utils::ensure_run_context;

impl Supervisor {
    /// Run a prompt synchronously (drains the session subscription).
    pub async fn run(&self, prompt: &str, opts: Option<OrchRunOptions>) -> OrchRunResult {
        let opts = ensure_run_context(opts);
        let target_agent = opts
            .command
            .target_agent_id
            .clone()
            .unwrap_or_else(|| "main".to_string());
        let host_context = opts
            .host_context
            .clone()
            .expect("run() requires host_context");
        let session_id = host_context.session_id.clone();
        let source_turn_id = opts
            .source_turn_id
            .clone()
            .unwrap_or_else(|| "turn_test".to_string());
        let work_id = opts
            .work_id
            .clone()
            .unwrap_or_else(super::utils::generate_work_id);

        let runtime = AgentRuntimeService::runtime_for(self);
        let subscription = runtime
            .start_root_turn(
                &session_id,
                &source_turn_id,
                &work_id,
                &target_agent,
                prompt,
                None,
                None,
            )
            .await
            .expect("start_root_turn must succeed for sync run helper");
        let mut output = subscription.output;

        let mut total_steps = 0;
        let mut status = RunStatus::Completed;
        let mut message_text_by_id: HashMap<String, String> = HashMap::new();
        let mut fallback_messages: Vec<(String, Message)> = Vec::new();
        let mut messages = Vec::new();
        let mut turn_done = false;
        let mut terminal_status: Option<TaskStatus> = None;

        while !turn_done {
            let Some(item) = output.next().await else {
                break;
            };
            let Ok(envelope) = item else {
                continue;
            };
            match envelope.output {
                SessionOutput::Delta(delta_envelope) => match delta_envelope.delta {
                    piko_protocol::agent_runtime::RealtimeDelta::Text { delta, .. } => {
                        if let Some(message_id) = delta_envelope.message_id {
                            message_text_by_id
                                .entry(message_id)
                                .or_default()
                                .push_str(&delta);
                        }
                    }
                    piko_protocol::agent_runtime::RealtimeDelta::MessageEnded {
                        stop_reason,
                        ..
                    } => {
                        if let Some(message_id) = delta_envelope.message_id
                            && let Some(text) = message_text_by_id.remove(&message_id)
                            && !text.is_empty()
                        {
                            fallback_messages.push((
                                message_id,
                                Message::Assistant {
                                    content: vec![ContentBlock::Text { text }],
                                    api: String::new(),
                                    provider: String::new(),
                                    model: String::new(),
                                    usage: None,
                                    stop_reason: stop_reason.clone(),
                                    error_message: None,
                                    timestamp: None,
                                },
                            ));
                        }
                    }
                    _ => {}
                },
                SessionOutput::Event(event_envelope) => {
                    if let piko_protocol::agent_runtime::SessionEvent::TaskChanged { snapshot } =
                        &event_envelope.event
                    {
                        match snapshot.status {
                            TaskStatus::Terminated | TaskStatus::Idle | TaskStatus::Failed => {
                                turn_done = true;
                                terminal_status = Some(snapshot.status.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if terminal_status.is_some() {
            let drain_deadline =
                tokio::time::Instant::now() + std::time::Duration::from_millis(200);
            while tokio::time::Instant::now() < drain_deadline {
                let remaining =
                    drain_deadline.saturating_duration_since(tokio::time::Instant::now());
                let Ok(Some(item)) = tokio::time::timeout(remaining, output.next()).await else {
                    break;
                };
                let Ok(envelope) = item else {
                    continue;
                };
                if let SessionOutput::Delta(delta_envelope) = envelope.output {
                    match delta_envelope.delta {
                        piko_protocol::agent_runtime::RealtimeDelta::Text { delta, .. } => {
                            if let Some(message_id) = delta_envelope.message_id {
                                message_text_by_id
                                    .entry(message_id)
                                    .or_default()
                                    .push_str(&delta);
                            }
                        }
                        piko_protocol::agent_runtime::RealtimeDelta::MessageEnded {
                            stop_reason,
                            ..
                        } => {
                            if let Some(message_id) = delta_envelope.message_id
                                && let Some(text) = message_text_by_id.remove(&message_id)
                                && !text.is_empty()
                            {
                                fallback_messages.push((
                                    message_id,
                                    Message::Assistant {
                                        content: vec![ContentBlock::Text { text }],
                                        api: String::new(),
                                        provider: String::new(),
                                        model: String::new(),
                                        usage: None,
                                        stop_reason: stop_reason.clone(),
                                        error_message: None,
                                        timestamp: None,
                                    },
                                ));
                                total_steps += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(snapshot_status) = terminal_status {
            status = match snapshot_status {
                TaskStatus::Failed => RunStatus::Error,
                TaskStatus::Terminated => RunStatus::Completed,
                _ => RunStatus::Completed,
            };
        }

        messages.extend(fallback_messages.into_iter().map(|(_, message)| message));
        if total_steps == 0 {
            total_steps = messages.len() as u32;
        }

        OrchRunResult {
            messages,
            total_steps,
            status,
        }
    }
}
