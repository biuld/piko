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
use crate::runtime::dispatch::{
    AgentDispatch, ChannelConfig, LifecycleDispatch, LifecycleEvent, SessionChannels,
};
use crate::runtime::stream::AgentRunDeps;
use crate::runtime::stream::{self, RunContext};
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};
use piko_protocol::{
    ContentBlock, Message, DisplayEvent, ServerMessage as Event, TaskEvent,
};

use super::supervisor::Supervisor;
use super::utils::{ensure_run_context, generate_task_id, run_status_from_final_status};

impl Supervisor {
    /// Run a prompt and return the host-facing event stream.
    pub async fn run_streaming(
        &self,
        prompt: &str,
        opts: Option<OrchRunOptions>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let mut channels = self.run_streaming_channels(prompt, opts).await;
        let mut persist = channels
            .persist_stream()
            .expect("persist stream must be available");
        let mut display = channels
            .display_stream()
            .expect("display stream must be available");

        Box::pin(async_stream::stream! {
            use crate::runtime::dispatch::server_message_from_persist_event;
            use crate::runtime::dispatch::server_message_from_display_event;

            let mut display_done = false;
            let mut persist_done = false;

            while !(display_done && persist_done) {
                tokio::select! {
                    biased;
                    display_event = display.next(), if !display_done => {
                        match display_event {
                            Some(event) => {
                                if let Some(msg) = server_message_from_display_event(event.as_ref()) {
                                    yield msg;
                                }
                            }
                            None => display_done = true,
                        }
                    }
                    persist_event = persist.next(), if !persist_done => {
                        match persist_event {
                            Some(event) => {
                                if let Some(msg) = server_message_from_persist_event(event.as_ref()) {
                                    yield msg;
                                }
                            }
                            None => persist_done = true,
                        }
                    }
                }
            }
        })
    }

    /// Run a prompt through the dispatch framework and return typed session channels.
    pub async fn run_streaming_channels(
        &self,
        prompt: &str,
        opts: Option<OrchRunOptions>,
    ) -> SessionChannels {
        use crate::runtime::stream::{self, RunContext};
        use crate::domain::tasks::task::{AgentTask, TaskSource};

        let target_agent = if let Some(aid) = opts
            .as_ref()
            .and_then(|o| o.command.target_agent_id.clone())
        {
            aid
        } else {
            self.state.default_agent_id.read().await.clone()
        };
        let task_id = format!(
            "task_{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(12)
                .collect::<String>()
        );
        let host_context = opts.as_ref().and_then(|o| o.host_context.clone());
        let session_id = host_context
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
        let ctx = RunContext {
            steer_tx: steer_tx.clone(),
            cancel: CancellationToken::new(),
        };
        *self.state.steer_tx.write().await = Some(steer_tx);

        let spawner: Arc<dyn AgentSpawner> = Arc::new(Self {
            state: Arc::clone(&self.state),
        });
        let root_stream =
            Box::pin(stream::agent_loop(ctx, steer_rx, deps, task, spec, spawner))
                as Pin<Box<dyn Stream<Item = Event> + Send>>;

        // Set up session channels
        let channels = SessionChannels::new(ChannelConfig::default());
        self.state
            .runtime_events
            .set(channels.persist_sender(), channels.display_sender());

        // Spawn dispatches and a cleanup task that clears the bus when all dispatches complete
        let bus = self.state.runtime_events.clone();
        let (lifecycle_tx, lifecycle_rx) = mpsc::unbounded_channel();
        let lifecycle_handle = channels.spawn_dispatch(
            LifecycleDispatch::new(session_id.clone(), lifecycle_rx),
            session_id.clone(),
        );

        let routed_event_stream = Box::pin(async_stream::stream! {
            let mut root_stream = root_stream;
            while let Some(event) = root_stream.next().await {
                match event {
                    Event::Display(piko_protocol::DisplayEvent::TaskLifecycle(event)) => {
                        let _ = lifecycle_tx.send(LifecycleEvent::Task(event));
                    }
                    Event::Display(piko_protocol::DisplayEvent::TurnLifecycle(event)) => {
                        let _ = lifecycle_tx.send(LifecycleEvent::Turn(event));
                    }
                    event => yield event,
                }
            }
        }) as Pin<Box<dyn Stream<Item = Event> + Send>>;

        let agent_handle = channels.spawn_dispatch(
            AgentDispatch::new(target_agent, routed_event_stream),
            session_id.clone(),
        );

        // Spawn a cleanup task that clears the channel bus after all dispatches complete,
        // so the channel receivers see EOF when the session is done.
        tokio::spawn(async move {
            let _ = lifecycle_handle.await;
            let _ = agent_handle.await;
            bus.clear();
        });

        channels
    }

    /// Run a prompt synchronously (drains the stream).
    pub async fn run(&self, prompt: &str, opts: Option<OrchRunOptions>) -> OrchRunResult {
        let stream = self
            .run_streaming(prompt, Some(ensure_run_context(opts)))
            .await;
        let mut total_steps = 0;
        let mut status = RunStatus::Completed;
        let mut message_text_by_id: HashMap<String, String> = HashMap::new();
        let mut fallback_messages: Vec<(String, Message)> = Vec::new();
        let mut messages = Vec::new();

        tokio::pin!(stream);
        while let Some(event) = stream.next().await {
            match event {
                Event::Display(DisplayEvent::TextDelta {
                    message_id, delta, ..
                }) => {
                    message_text_by_id
                        .entry(message_id)
                        .or_default()
                        .push_str(&delta);
                }
                Event::Display(DisplayEvent::MessageEnd {
                    message_id,
                    stop_reason,
                    ..
                }) => {
                    if let Some(text) = message_text_by_id.remove(&message_id)
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
                                stop_reason,
                                error_message: None,
                                timestamp: None,
                            },
                        ));
                    }
                }
                Event::Display(DisplayEvent::AssistantCompleted {
                    message_id,
                    message,
                    ..
                }) => {
                    fallback_messages.retain(|(id, _)| id != &message_id);
                    messages.push(message);
                }
                Event::Display(piko_protocol::DisplayEvent::TaskLifecycle(TaskEvent::Completed {
                    total_steps: steps,
                    final_status,
                    ..
                })) => {
                    total_steps = steps;
                    status = run_status_from_final_status(&final_status);
                }
                Event::Display(piko_protocol::DisplayEvent::TaskLifecycle(TaskEvent::Failed { .. })) => status = RunStatus::Error,
                Event::Display(piko_protocol::DisplayEvent::TaskLifecycle(TaskEvent::Cancelled { .. })) => status = RunStatus::Aborted,
                _ => {}
            }
        }

        messages.extend(fallback_messages.into_iter().map(|(_, message)| message));

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
        parent_agent_id: Option<piko_protocol::AgentId>,
        parent_task_id: Option<String>,
        task_id: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let agent_id = spec.id.clone();
        let task_id = task_id.unwrap_or_else(generate_task_id);
        let cancel = CancellationToken::new();
        let (steer_tx, steer_rx) = tokio::sync::mpsc::unbounded_channel();

        self.state
            .dag
            .write()
            .await
            .insert(agent_id.clone(), parent_agent_id.clone());
        self.state.handles.write().await.insert(
            agent_id.clone(),
            super::supervisor::AgentHandle {
                agent_id: agent_id.clone(),
                parent_agent_id: parent_agent_id.clone(),
                cancel: cancel.clone(),
                steer_tx: steer_tx.clone(),
            },
        );

        let task = AgentTask {
            id: Some(task_id),
            target_agent_id: agent_id,
            prompt,
            source: TaskSource::User,
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

        Box::pin(stream::agent_loop(ctx, steer_rx, deps, task, spec, spawner))
    }
}