// ---- Run — streaming and synchronous run methods ----

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_core::Stream;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext, TaskSource};
use crate::ports::agent_spawner::AgentSpawner;
use crate::runtime::stream::AgentRunDeps;
use crate::runtime::stream::{self, RunContext};
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};
use piko_protocol::{ContentBlock, Event, Message};

use super::supervisor::Supervisor;
use super::utils::{ensure_run_context, generate_task_id, run_status_from_final_status};

enum RunStreamItem {
    Root(Event),
    Fanout(Event),
}

impl Supervisor {
    /// Run a prompt and return the host-facing event stream.
    pub async fn run_streaming(
        &self,
        prompt: &str,
        opts: Option<OrchRunOptions>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
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

        let supervisor = Self {
            state: Arc::clone(&self.state),
        };
        let spawner: Arc<dyn AgentSpawner> = Arc::new(Self {
            state: Arc::clone(&self.state),
        });
        let root_stream = Box::pin(stream::agent_loop(ctx, steer_rx, deps, task, spec, spawner))
            as Pin<Box<dyn Stream<Item = Event> + Send>>;

        Box::pin(async_stream::stream! {
            let mut root_stream = root_stream;
            let mut runtime_events = supervisor.state.runtime_events.subscribe().await;
            let mut root_done = false;
            let mut active_children = HashSet::<String>::new();
            let current_host_context = host_context;

            loop {
                let next = if root_done && active_children.is_empty() {
                    match tokio::time::timeout(Duration::from_millis(1), runtime_events.next()).await {
                        Ok(Some(event)) => RunStreamItem::Fanout(event),
                        _ => break,
                    }
                } else {
                    tokio::select! {
                        root = root_stream.next(), if !root_done => {
                            match root {
                                Some(event) => RunStreamItem::Root(event),
                                None => {
                                    root_done = true;
                                    continue;
                                }
                            }
                        }
                        event = runtime_events.next() => {
                            match event {
                                Some(event) => RunStreamItem::Fanout(event),
                                None => {
                                    if root_done {
                                        break;
                                    }
                                    continue;
                                }
                            }
                        }
                    }
                };

                match next {
                    RunStreamItem::Root(event) => {
                        supervisor.observe_task_event(&event).await;
                        yield event;
                    }
                    RunStreamItem::Fanout(event) => {
                        if let Event::TaskCreated {
                            task_id,
                            parent_task_id,
                            session_id,
                            turn_id,
                            ..
                        } = &event
                        {
                            let belongs_to_run = match current_host_context.as_ref() {
                                Some(current) => {
                                    current.session_id == *session_id && current.turn_id == *turn_id
                                }
                                None => true,
                            };
                            if belongs_to_run && parent_task_id.is_some() {
                                active_children.insert(task_id.clone());
                                yield event;
                            }
                            continue;
                        }

                        let Some(task_id) = event_task_id(&event) else {
                            continue;
                        };
                        if !active_children.contains(task_id) {
                            continue;
                        }

                        match &event {
                            Event::TaskCompleted { task_id, .. }
                            | Event::TaskFailed { task_id, .. }
                            | Event::TaskCancelled { task_id, .. } => {
                                active_children.remove(task_id);
                            }
                            _ => {}
                        }
                        yield event;
                    }
                }
            }
        })
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
                Event::TextDelta {
                    message_id, delta, ..
                } => {
                    message_text_by_id
                        .entry(message_id)
                        .or_default()
                        .push_str(&delta);
                }
                Event::MessageEnd {
                    message_id,
                    stop_reason,
                    ..
                } => {
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
                Event::AssistantMessageCompleted {
                    message_id,
                    message,
                    ..
                } => {
                    fallback_messages.retain(|(id, _)| id != &message_id);
                    messages.push(message);
                }
                Event::TaskCompleted {
                    total_steps: steps,
                    final_status,
                    ..
                } => {
                    total_steps = steps;
                    status = run_status_from_final_status(&final_status);
                }
                Event::TaskFailed { .. } => status = RunStatus::Error,
                Event::TaskCancelled { .. } => status = RunStatus::Aborted,
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

fn event_task_id(event: &Event) -> Option<&str> {
    match event {
        Event::UserMessageSubmitted { task_id, .. }
        | Event::AssistantMessageCompleted { task_id, .. }
        | Event::ToolResultCommitted { task_id, .. }
        | Event::TurnStarted {
            root_task_id: task_id,
            ..
        }
        | Event::TaskCreated { task_id, .. }
        | Event::TaskStarted { task_id, .. }
        | Event::TaskCompleted { task_id, .. }
        | Event::TaskFailed { task_id, .. }
        | Event::TaskCancelled { task_id, .. }
        | Event::TaskJoined { task_id, .. }
        | Event::TaskSteered { task_id, .. }
        | Event::MessageStart { task_id, .. }
        | Event::TextDelta { task_id, .. }
        | Event::ThinkingDelta { task_id, .. }
        | Event::MessageEnd { task_id, .. }
        | Event::ToolStart { task_id, .. }
        | Event::ToolEnd { task_id, .. } => Some(task_id),
        _ => None,
    }
}
