// ---- Run — streaming and synchronous run methods ----

use std::collections::HashMap;
use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext, TaskSource};
use crate::runtime::dispatch::SessionChannels;
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};
use piko_protocol::{ContentBlock, DisplayEvent, Message, ServerMessage as Event, TaskEvent};

use super::supervisor::Supervisor;
use super::task_driver::spawn_task_driver;
use super::task_launcher::{
    root_session_channels, spawn_registered_agent_stream, try_reuse_root_task,
};
use super::utils::{ensure_run_context, generate_task_id, run_status_from_final_status};

impl Supervisor {
    /// Run a prompt through the dispatch framework and return typed session channels.
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
            .map(|context| context.session_id.as_str())
            .unwrap_or(&self.state.run_id);

        if let Some(channels) = try_reuse_root_task(
            std::sync::Arc::clone(&self.state),
            &target_agent,
            prompt,
            session_id,
        )
        .await
        {
            return channels;
        }

        let task_id = format!(
            "task_{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(12)
                .collect::<String>()
        );
        let spec = self.ensure_agent(&target_agent).await;

        let channels =
            root_session_channels(std::sync::Arc::clone(&self.state), host_context.as_ref());
        let senders = channels.senders();

        let task = AgentTask {
            id: Some(task_id.clone()),
            target_agent_id: target_agent.clone(),
            prompt: prompt.to_string(),
            source: TaskSource::User,
            priority: None,
            parent_task_id: None,
            history: opts.as_ref().and_then(|o| o.history.clone()),
            host_context: host_context.clone(),
        };
        let root_stream =
            spawn_registered_agent_stream(self, spec, task, Some(senders), true).await;

        spawn_task_driver(
            std::sync::Arc::clone(&self.state),
            task_id,
            root_stream,
            None,
        );

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
