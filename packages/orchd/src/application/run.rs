// ---- Run — streaming and synchronous run methods ----

use std::collections::HashMap;
use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;
use piko_protocol::agent_runtime::{CreateTaskRequest, InputSource, SubscribeRequest, TaskMode};
use piko_protocol::agent_runtime::{SessionOutput, TaskStatus};
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};
use piko_protocol::{ContentBlock, Message};

use crate::api::{AgentRuntime, SessionSubscription};
use crate::application::service::AgentRuntimeService;
use crate::domain::tasks::task::HostTaskContext;
use crate::runtime::orchestrator::input::build_user_input;

use super::supervisor::Supervisor;
use super::utils::{ensure_run_context, generate_task_id};

impl Supervisor {
    /// Run a prompt through the unified Agent API and return a session subscription.
    pub async fn run_streaming_subscription(
        &self,
        prompt: &str,
        opts: Option<OrchRunOptions>,
    ) -> SessionSubscription {
        let target_agent = if let Some(aid) = opts
            .as_ref()
            .and_then(|o| o.command.target_agent_id.clone())
        {
            aid
        } else {
            self.state.default_agent_id.read().await.clone()
        };
        let host_context = opts.as_ref().and_then(|o| o.host_context.clone());
        let session_id = host_context
            .as_ref()
            .map(|context| context.session_id.clone())
            .unwrap_or_else(|| self.state.run_id.clone());
        let work_id = host_context
            .as_ref()
            .map(|context| context.turn_id.clone())
            .unwrap_or_else(|| format!("work_{}", uuid::Uuid::new_v4()));

        let runtime = AgentRuntimeService::runtime_for(self);
        let subscription = runtime
            .subscribe_session(SubscribeRequest {
                session_id: session_id.clone(),
                task_id: None,
                after: None,
            })
            .await
            .expect("session subscription must be available");

        if let Some(task_id) = self
            .state
            .registry
            .active_root_task_for_agent(&target_agent, &session_id)
            .await
        {
            if runtime
                .submit_input(build_user_input(
                    &session_id,
                    &task_id,
                    &work_id,
                    piko_protocol::MessageContent::String(prompt.to_string()),
                    InputSource::User,
                ))
                .await
                .is_ok()
            {
                return subscription;
            }
            self.state.registry.cleanup_runtime(&task_id).await;
        }

        let _ = self.ensure_agent(&target_agent).await;
        let task_id = generate_task_id();
        let request = CreateTaskRequest {
            request_id: format!("req_{}", uuid::Uuid::new_v4()),
            session_id: session_id.clone(),
            task_id: Some(task_id.clone()),
            agent_id: target_agent,
            parent_task_id: None,
            source: InputSource::User,
            mode: TaskMode::Attached,
            host_context: host_context.clone().unwrap_or(HostTaskContext {
                session_id: session_id.clone(),
                turn_id: work_id.clone(),
            }),
            initial_history: opts.as_ref().and_then(|o| o.history.clone()),
        };

        if runtime.create_task(request).await.is_ok() {
            let _ = runtime
                .submit_input(build_user_input(
                    &session_id,
                    &task_id,
                    &work_id,
                    piko_protocol::MessageContent::String(prompt.to_string()),
                    InputSource::User,
                ))
                .await;
        }

        subscription
    }

    /// Run a prompt synchronously (drains the session subscription).
    pub async fn run(&self, prompt: &str, opts: Option<OrchRunOptions>) -> OrchRunResult {
        let subscription = self
            .run_streaming_subscription(prompt, Some(ensure_run_context(opts)))
            .await;
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

    /// Spawn the root agent and return its event stream.
    pub async fn spawn_root_agent(
        &self,
        spec: crate::domain::agents::spec::AgentSpec,
        prompt: String,
        host_context: Option<HostTaskContext>,
    ) -> Pin<Box<dyn Stream<Item = piko_protocol::ServerMessage> + Send>> {
        self.spawn_agent_stream(spec, prompt, host_context, None, None, None, false)
            .await
    }

    /// Internal: create an agent stream and wire it into the DAG.
    pub(crate) async fn spawn_agent_stream(
        &self,
        spec: crate::domain::agents::spec::AgentSpec,
        prompt: String,
        host_context: Option<HostTaskContext>,
        source_agent_id: Option<piko_protocol::AgentId>,
        parent_task_id: Option<String>,
        task_id: Option<String>,
        allow_followup_turns: bool,
    ) -> Pin<Box<dyn Stream<Item = piko_protocol::ServerMessage> + Send>> {
        use super::task_launcher::spawn_registered_agent_stream;
        use crate::domain::tasks::task::{AgentTask, TaskSource};

        let agent_id = spec.id.clone();
        let task_id = task_id.unwrap_or_else(generate_task_id);

        let source = match (&source_agent_id, &parent_task_id) {
            (Some(agent_id), Some(task_id)) => TaskSource::Agent {
                agent_id: agent_id.clone(),
                task_id: task_id.clone(),
            },
            _ => TaskSource::User,
        };

        let task = AgentTask {
            id: Some(task_id),
            target_agent_id: agent_id,
            prompt,
            source,
            priority: None,
            parent_task_id,
            history: None,
            host_context,
        };
        spawn_registered_agent_stream(self, spec, task, allow_followup_turns).await
    }
}
