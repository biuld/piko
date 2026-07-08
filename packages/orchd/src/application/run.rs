// ---- Run — streaming and synchronous run methods ----

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext, TaskSource};
use crate::ports::agent_spawner::AgentSpawner;
use crate::runtime::dispatch::{ChannelConfig, SessionChannels};
use crate::runtime::stream::AgentRunDeps;
use crate::runtime::stream::{self, RunContext};
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};
use piko_protocol::{ContentBlock, DisplayEvent, Message, ServerMessage as Event, TaskEvent};

use super::supervisor::Supervisor;
use super::utils::{ensure_run_context, generate_task_id, run_status_from_final_status};

impl Supervisor {
    /// Run a prompt through the dispatch framework and return typed session channels.
    pub async fn run_streaming_channels(
        &self,
        prompt: &str,
        opts: Option<OrchRunOptions>,
    ) -> SessionChannels {
        use crate::domain::tasks::task::{AgentTask, TaskSource};
        use crate::runtime::stream::{self, RunContext};

        let target_agent = if let Some(aid) = opts
            .as_ref()
            .and_then(|o| o.command.target_agent_id.clone())
        {
            aid
        } else {
            self.state.default_agent_id.read().await.clone()
        };

        let existing_task_id = {
            let tasks = self.state.tasks.read().await;
            tasks
                .values()
                .find(|t| {
                    t.target_agent_id == target_agent
                        && t.parent_task_id.is_none()
                        && !matches!(
                            t.status,
                            crate::domain::tasks::task::AgentTaskStatus::Completed
                                | crate::domain::tasks::task::AgentTaskStatus::Failed
                                | crate::domain::tasks::task::AgentTaskStatus::Cancelled
                        )
                })
                .map(|t| t.id.clone())
        };

        if let Some(tid) = existing_task_id {
            let handle = self.state.handles.read().await.get(&tid).cloned();
            if let Some(handle) = handle {
                let mut channels = SessionChannels::new(ChannelConfig::default());
                let session_id = self.state.run_id.clone();
                channels.spawn_lifecycle_dispatch(session_id);
                let senders = channels.senders();

                let _ = handle
                    .steer_tx
                    .send(crate::domain::tasks::steering::SteerMessage {
                        source_task_id: String::new(),
                        source_agent_id: String::new(),
                        message: prompt.to_string(),
                        senders: Some(senders),
                    });

                return channels;
            }
        }
        let task_id = format!(
            "task_{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(12)
                .collect::<String>()
        );
        let host_context = opts.as_ref().and_then(|o| o.host_context.clone());
        let _session_id = host_context
            .as_ref()
            .map(|ctx| ctx.session_id.clone())
            .unwrap_or_default();
        let spec = self.ensure_agent(&target_agent).await;

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

        let deps = AgentRunDeps {
            model_executor: Arc::clone(&self.state.model_executor),
            model_config: self.state.model_config.read().await.clone(),
            tool_registry: Arc::clone(&self.state.tool_registry),
        };

        let (steer_tx, steer_rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let ctx = RunContext {
            steer_tx: steer_tx.clone(),
            cancel: cancel.clone(),
        };
        *self.state.steer_tx.write().await = Some(steer_tx);
        self.state
            .task_dag
            .write()
            .await
            .insert(task_id.clone(), None);
        self.state.handles.write().await.insert(
            task_id.clone(),
            super::supervisor::AgentHandle {
                task_id: task_id.clone(),
                agent_id: target_agent.clone(),
                parent_task_id: None,
                cancel,
                steer_tx: ctx.steer_tx.clone(),
            },
        );

        let spawner: Arc<dyn AgentSpawner> = Arc::new(Self {
            state: Arc::clone(&self.state),
        });

        // Set up session channels
        let mut channels = SessionChannels::new(ChannelConfig::default());
        let session_id = host_context
            .as_ref()
            .map(|ctx| ctx.session_id.clone())
            .unwrap_or_else(|| self.state.run_id.clone());
        channels.spawn_lifecycle_dispatch(session_id);
        let senders = channels.senders();

        let root_stream = Box::pin(stream::agent_loop(
            ctx,
            steer_rx,
            deps,
            task,
            spec,
            spawner,
            Some(senders),
        )) as Pin<Box<dyn Stream<Item = Event> + Send>>;

        // Drain the root stream to completion.
        // All events are dispatched directly through DispatchSenders —
        // the stream only yields when senders is None (tests/standalone).
        tokio::spawn(async move {
            root_stream.collect::<Vec<_>>().await;
        });

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
                    println!("Task failed in run(): {error}");
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
        self.spawn_agent_stream(spec, prompt, host_context, None, None, None)
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
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let agent_id = spec.id.clone();
        let task_id = task_id.unwrap_or_else(generate_task_id);
        let cancel = CancellationToken::new();
        let (steer_tx, steer_rx) = tokio::sync::mpsc::unbounded_channel();

        self.state
            .task_dag
            .write()
            .await
            .insert(task_id.clone(), parent_task_id.clone());
        self.state.handles.write().await.insert(
            task_id.clone(),
            super::supervisor::AgentHandle {
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                parent_task_id: parent_task_id.clone(),
                cancel: cancel.clone(),
                steer_tx: steer_tx.clone(),
            },
        );

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

        let deps = AgentRunDeps {
            model_executor: Arc::clone(&self.state.model_executor),
            model_config: self.state.model_config.read().await.clone(),
            tool_registry: Arc::clone(&self.state.tool_registry),
        };

        let ctx = RunContext {
            steer_tx: steer_tx.clone(),
            cancel: cancel.clone(),
        };

        let spawner: Arc<dyn AgentSpawner> = Arc::new(Self {
            state: Arc::clone(&self.state),
        });

        Box::pin(stream::agent_loop(
            ctx, steer_rx, deps, task, spec, spawner, None,
        ))
    }
}
