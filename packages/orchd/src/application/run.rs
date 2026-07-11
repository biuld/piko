// ---- Run — streaming and synchronous run methods ----

use std::collections::HashMap;
use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;

use crate::application::service::AgentRuntimeService;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext, TaskSource};
use crate::runtime::dispatch::SessionChannels;
use crate::runtime::orchestrator::input::build_user_input;
use piko_protocol::agent_runtime::{CreateTaskRequest, InputSource, TaskMode};
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};
use piko_protocol::{ContentBlock, DisplayEvent, Message, ServerMessage as Event, TaskEvent};

use super::supervisor::Supervisor;
use super::task_launcher::{root_session_channels, spawn_registered_agent_stream};
use super::utils::{ensure_run_context, generate_task_id, run_status_from_final_status};

impl Supervisor {
    /// Run a prompt through the unified Agent API and return typed session channels.
    pub async fn run_streaming_channels(
        &self,
        prompt: &str,
        opts: Option<OrchRunOptions>,
    ) -> SessionChannels {
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

        let channels =
            root_session_channels(std::sync::Arc::clone(&self.state), host_context.as_ref());
        let runtime = AgentRuntimeService::runtime_for(self);

        if let Some(task_id) = self
            .state
            .registry
            .active_root_task_for_agent(&target_agent, &session_id)
            .await
        {
            if runtime
                .submit_input_with_senders(
                    build_user_input(
                        &session_id,
                        &task_id,
                        &work_id,
                        piko_protocol::MessageContent::String(prompt.to_string()),
                        InputSource::User,
                    ),
                    Some(channels.senders()),
                )
                .await
                .is_ok()
            {
                return channels;
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

        if runtime
            .create_task_with_senders(request, Some(channels.senders()))
            .await
            .is_ok()
        {
            let _ = runtime
                .submit_input_with_senders(
                    build_user_input(
                        &session_id,
                        &task_id,
                        &work_id,
                        piko_protocol::MessageContent::String(prompt.to_string()),
                        InputSource::User,
                    ),
                    Some(channels.senders()),
                )
                .await;
        }

        channels
    }

    /// Run a prompt synchronously (drains the stream).
    pub async fn run(&self, prompt: &str, opts: Option<OrchRunOptions>) -> OrchRunResult {
        let mut channels = self
            .run_streaming_channels(prompt, Some(ensure_run_context(opts)))
            .await;
        let mut display = channels.display_stream().unwrap();
        let mut lifecycle = channels.lifecycle_stream().unwrap();
        let mut persist = channels.persist_stream().unwrap();
        drop(channels);

        tokio::spawn(async move { while persist.next().await.is_some() {} });

        let mut total_steps = 0;
        let mut status = RunStatus::Completed;

        let display_handle = tokio::spawn(async move {
            let mut message_text_by_id: HashMap<String, String> = HashMap::new();
            let mut fallback_messages: Vec<(String, Message)> = Vec::new();
            let mut messages = Vec::new();

            while let Some(event) = display.next().await {
                match event.as_ref() {
                    DisplayEvent::TextDelta {
                        message_id, delta, ..
                    } => {
                        message_text_by_id
                            .entry(message_id.clone())
                            .or_default()
                            .push_str(delta);
                    }
                    DisplayEvent::MessageEnd {
                        message_id,
                        stop_reason,
                        ..
                    } => {
                        if let Some(text) = message_text_by_id.remove(message_id)
                            && !text.is_empty()
                        {
                            fallback_messages.push((
                                message_id.clone(),
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
                    DisplayEvent::Finalized {
                        message_id,
                        content,
                        usage,
                        stop_reason,
                        error_message,
                        ..
                    } => {
                        fallback_messages.retain(|(id, _)| id != message_id);
                        messages.push(Message::Assistant {
                            content: content.clone(),
                            api: String::new(),
                            provider: String::new(),
                            model: String::new(),
                            usage: usage.clone(),
                            stop_reason: stop_reason.clone(),
                            error_message: error_message.clone(),
                            timestamp: None,
                        });
                    }
                    _ => {}
                }
            }
            messages.extend(fallback_messages.into_iter().map(|(_, message)| message));
            messages
        });

        while let Some(event) = lifecycle.next().await {
            match event.as_ref() {
                crate::runtime::dispatch::LifecycleEvent::Task(TaskEvent::Completed {
                    total_steps: steps,
                    final_status,
                    ..
                }) => {
                    total_steps = *steps;
                    status = run_status_from_final_status(final_status);
                }
                crate::runtime::dispatch::LifecycleEvent::Task(TaskEvent::Idle {
                    total_steps: steps,
                    ..
                }) => {
                    total_steps = *steps;
                    status = RunStatus::Completed;
                }
                crate::runtime::dispatch::LifecycleEvent::Task(TaskEvent::Failed {
                    error, ..
                }) => {
                    tracing::error!(%error, "task failed during synchronous run");
                    status = RunStatus::Error;
                }
                crate::runtime::dispatch::LifecycleEvent::Task(TaskEvent::Cancelled { .. }) => {
                    status = RunStatus::Aborted
                }
                _ => {}
            }
        }

        let messages = display_handle.await.unwrap_or_default();

        OrchRunResult {
            messages,
            total_steps,
            status,
        }
    }

    /// Spawn the root agent and return its event stream.
    pub async fn spawn_root_agent(
        &self,
        spec: AgentSpec,
        prompt: String,
        host_context: Option<HostTaskContext>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        self.spawn_agent_stream(spec, prompt, host_context, None, None, None, false)
            .await
    }

    /// Internal: create an agent stream and wire it into the DAG.
    pub(crate) async fn spawn_agent_stream(
        &self,
        spec: AgentSpec,
        prompt: String,
        host_context: Option<HostTaskContext>,
        source_agent_id: Option<piko_protocol::AgentId>,
        parent_task_id: Option<String>,
        task_id: Option<String>,
        allow_followup_turns: bool,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
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
        spawn_registered_agent_stream(self, spec, task, None, allow_followup_turns).await
    }
}
