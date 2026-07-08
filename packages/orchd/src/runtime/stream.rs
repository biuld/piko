// ---- agent_task_stream — async-stream based root agent execution ----

use std::sync::Arc;

use async_stream::stream;
use futures_core::Stream;
use llmd::gateway::GatewayRequest;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::ToolRegistry;
use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::events::event::Event;
use crate::domain::model::step::{ModelConfig, ModelRunSettings, ModelSpec};
use crate::domain::model::transcript::{ContentBlock, Message, MessageContent};
use crate::domain::tasks::steering::SteerMessage;
use crate::domain::tasks::task::AgentTask;
use crate::ports::agent_spawner::AgentSpawner;
use crate::ports::model_gateway::LlmGateway;
use crate::ports::tool_provider::ToolDiscoveryContext;

use super::dispatch::{
    StepDispatch, StepDispatchResult, TaskLifecycleDispatcher, ToolExecutionConsumer,
};
use super::tool_executor;

// ---- Agent run dependencies ----

/// Dependencies injected into an agent run.
#[derive(Clone)]
pub(crate) struct AgentRunDeps {
    pub model_executor: Arc<dyn LlmGateway>,
    pub model_config: Option<ModelConfig>,
    pub tool_registry: Arc<ToolRegistryImpl>,
}

// ---- Transcript manager ----

pub(crate) struct TranscriptManager {
    messages: Vec<Message>,
}

impl TranscriptManager {
    pub(crate) fn new(history: Option<Vec<Message>>) -> Self {
        Self {
            messages: history.unwrap_or_default(),
        }
    }

    pub(crate) fn push_user(&mut self, text: String) {
        if !text.trim().is_empty() {
            self.messages.push(Message::User {
                content: MessageContent::String(text),
                timestamp: None,
            });
        }
    }

    pub(crate) fn push_assistant(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub(crate) fn push_tool_calls(
        &mut self,
        tool_calls: &[super::tool_calls::ToolCallItem],
        model_id: &str,
        provider: &str,
    ) {
        for tc in tool_calls {
            self.messages.push(Message::ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                model: Some(model_id.to_string()),
                provider: Some(provider.to_string()),
                timestamp: Some(now_ms()),
            });
        }
    }

    pub(crate) fn push_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub(crate) fn to_vec(&self) -> Vec<Message> {
        self.messages.clone()
    }
}

// ---- Per-run context ----

pub(crate) struct RunContext {
    #[allow(dead_code)] // held to keep channel alive
    pub steer_tx: mpsc::UnboundedSender<SteerMessage>,
    pub cancel: CancellationToken,
}

struct StepCycle {
    result: StepDispatchResult,
    routes: std::collections::HashMap<String, crate::adapters::tools::registry::CatalogRoute>,
    model: ModelSpec,
    message_id: String,
}

struct PendingToolExecution {
    tool_calls: Vec<super::tool_calls::ToolCallItem>,
    routes: std::collections::HashMap<String, crate::adapters::tools::registry::CatalogRoute>,
    message_id: String,
}

enum StepAdvance {
    AwaitNextTurn {
        events: Vec<Event>,
        summary: String,
    },
    ExecuteTools {
        events: Vec<Event>,
        pending: PendingToolExecution,
    },
}

struct TaskOrchestrator {
    ctx: RunContext,
    deps: AgentRunDeps,
    spec: AgentSpec,
    task: AgentTask,
    spawner: Arc<dyn AgentSpawner>,
    senders: Option<crate::runtime::dispatch::DispatchSenders>,
    lifecycle_dispatcher: TaskLifecycleDispatcher,
    transcript: TranscriptManager,
    model_settings: ModelRunSettings,
    model_config: Option<ModelConfig>,
    steer_rx: mpsc::UnboundedReceiver<SteerMessage>,
    step_count: u32,
    task_id: String,
    agent_id: String,
    host_context: Option<crate::domain::tasks::task::HostTaskContext>,
    source_agent_id: Option<String>,
}

impl TaskOrchestrator {
    fn new(
        ctx: RunContext,
        steer_rx: mpsc::UnboundedReceiver<SteerMessage>,
        deps: AgentRunDeps,
        task: AgentTask,
        spec: AgentSpec,
        spawner: Arc<dyn AgentSpawner>,
        senders: Option<crate::runtime::dispatch::DispatchSenders>,
    ) -> Self {
        let task_id = task.id.clone().unwrap_or_default();
        let agent_id = spec.id.clone();
        let host_context = task.host_context.clone();
        let source_agent_id = match &task.source {
            piko_protocol::agents::TaskSource::Agent { agent_id, .. } => Some(agent_id.clone()),
            _ => None,
        };
        let mut transcript = TranscriptManager::new(task.history.clone());
        transcript.push_user(task.prompt.clone());
        let model_settings = deps
            .model_config
            .as_ref()
            .map(|c| c.settings.clone())
            .unwrap_or(ModelRunSettings {
                allow_tool_calls: true,
                ..Default::default()
            });
        let model_config = deps.model_config.clone();
        let lifecycle_dispatcher = TaskLifecycleDispatcher::new(
            senders.clone(),
            host_context.clone(),
            task_id.clone(),
            agent_id.clone(),
        );

        Self {
            ctx,
            deps,
            spec,
            task,
            spawner,
            senders,
            lifecycle_dispatcher,
            transcript,
            model_settings,
            model_config,
            steer_rx,
            step_count: 0,
            task_id,
            agent_id,
            host_context,
            source_agent_id,
        }
    }

    async fn initialize_events(&self) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some(ev) = self
            .lifecycle_dispatcher
            .created(
                self.task.parent_task_id.clone(),
                self.source_agent_id.clone(),
                self.task.prompt.clone(),
                self.host_context
                    .as_ref()
                    .map(|hc| hc.turn_id.clone())
                    .unwrap_or_default(),
            )
            .await
        {
            events.push(ev);
        }
        if let Some(ev) = self.lifecycle_dispatcher.started().await {
            events.push(ev);
        }
        events
    }

    async fn cancelled_event(&self) -> Option<Event> {
        self.lifecycle_dispatcher.cancelled().await
    }

    async fn drain_pending_steers(&mut self) -> Vec<Event> {
        drain_steering_messages(
            &mut self.steer_rx,
            &mut self.transcript,
            &self.lifecycle_dispatcher,
        )
        .await
    }

    fn current_model(&self) -> ModelSpec {
        self.model_config
            .as_ref()
            .map(|c| c.model.clone())
            .unwrap_or(ModelSpec {
                id: "default".into(),
                name: "Default".into(),
                provider: "openai".into(),
            })
    }

    async fn run_step_cycle(&mut self) -> Result<StepCycle, StepDispatchFailure> {
        self.step_count += 1;
        let (tools, routes) = (*self.deps.tool_registry)
            .discover_tools(&ToolDiscoveryContext {
                agent_id: self.agent_id.clone(),
                task_id: Some(self.task_id.clone()),
                tool_set_ids: self.spec.tool_set_ids.clone(),
                active_tool_names: self.spec.active_tool_names.clone(),
            })
            .await;
        let model = self.current_model();
        let message_id =
            runtime_assistant_message_id(&self.task_id, &format!("step_{}", self.step_count));
        let result = run_step_dispatch(
            &self.deps,
            &self.ctx,
            &self.transcript,
            &self.spec,
            &self.host_context,
            &self.task_id,
            &self.agent_id,
            model.clone(),
            message_id.clone(),
            format!("step_{}", self.step_count),
            tools,
            self.senders.as_ref(),
        )
        .await?;

        Ok(StepCycle {
            result,
            routes,
            model,
            message_id,
        })
    }

    async fn wait_for_next_turn(&mut self, summary: String) -> (Vec<Event>, bool) {
        let mut events = Vec::new();
        self.senders = None;
        self.lifecycle_dispatcher = TaskLifecycleDispatcher::new(
            None,
            self.host_context.clone(),
            self.task_id.clone(),
            self.agent_id.clone(),
        );

        match wait_for_next_turn_input(&self.ctx, &mut self.steer_rx).await {
            Some(msg) => {
                let next_senders = msg.senders;
                let next_dispatcher = TaskLifecycleDispatcher::new(
                    next_senders.clone(),
                    self.host_context.clone(),
                    self.task_id.clone(),
                    self.agent_id.clone(),
                );

                self.transcript.push_user(msg.message.clone());
                if let Some(ev) = next_dispatcher
                    .steered(msg.source_task_id, msg.source_agent_id, msg.message)
                    .await
                {
                    events.push(ev);
                }
                if let Some(ev) = next_dispatcher.started().await {
                    events.push(ev);
                }

                self.senders = next_senders;
                self.lifecycle_dispatcher = next_dispatcher;
                (events, true)
            }
            None => {
                if self.ctx.cancel.is_cancelled() {
                    if let Some(ev) = self.lifecycle_dispatcher.cancelled().await {
                        events.push(ev);
                    }
                } else if let Some(ev) = self
                    .lifecycle_dispatcher
                    .completed(self.step_count, summary)
                    .await
                {
                    events.push(ev);
                }
                (events, false)
            }
        }
    }

    async fn execute_tool_calls(
        &mut self,
        tool_calls: &[super::tool_calls::ToolCallItem],
        routes: &std::collections::HashMap<String, crate::adapters::tools::registry::CatalogRoute>,
        message_id: String,
    ) -> Result<tool_executor::ToolExecutionResult, String> {
        let tool_consumer = ToolExecutionConsumer::new(
            self.senders.clone(),
            self.host_context.clone(),
            self.task_id.clone(),
            self.agent_id.clone(),
            message_id,
        );
        tool_consumer
            .execute_tool_calls(
                &self.deps,
                &self.spawner,
                tool_calls,
                routes,
                &self.model_settings,
                self.ctx.cancel.clone(),
                &mut self.transcript,
                self.step_count,
            )
            .await
    }

    async fn handle_step_failure(&mut self, failure: StepDispatchFailure) -> Vec<Event> {
        let StepDispatchFailure { error, result } = failure;
        let StepDispatchResult {
            display_events,
            persist_events,
            ..
        } = result;
        let mut events = local_collected_step_events(&self.senders, display_events, persist_events);
        if let Some(ev) = self
            .lifecycle_dispatcher
            .failed(format!("Gateway error: {error}"))
            .await
        {
            events.push(ev);
        }
        events
    }

    async fn advance_after_step(&mut self, cycle: StepCycle) -> StepAdvance {
        let StepCycle {
            result,
            routes,
            model,
            message_id,
        } = cycle;
        let StepDispatchResult {
            assistant_message,
            tool_calls,
            display_events,
            persist_events,
        } = result;

        let mut events = local_collected_step_events(&self.senders, display_events, persist_events);
        self.transcript.push_assistant(assistant_message.clone());
        self.transcript
            .push_tool_calls(&tool_calls, &model.id, &model.provider);

        if tool_calls.is_empty() || !self.model_settings.allow_tool_calls {
            let summary = summarize(&assistant_message);
            if let Some(ev) = self
                .lifecycle_dispatcher
                .idle(self.step_count, summary.clone())
                .await
            {
                events.push(ev);
            }
            StepAdvance::AwaitNextTurn { events, summary }
        } else {
            StepAdvance::ExecuteTools {
                events,
                pending: PendingToolExecution {
                    tool_calls,
                    routes,
                    message_id,
                },
            }
        }
    }
}

// ---- Pure helpers (no yield) ----

fn summarize(msg: &Message) -> String {
    let text: String = match msg {
        Message::Assistant { content, .. } => content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    if text.len() > 200 {
        format!("{}...", &text[..200])
    } else {
        text
    }
}

fn local_collected_step_events(
    senders: &Option<crate::runtime::dispatch::DispatchSenders>,
    display_events: Vec<crate::runtime::dispatch::DisplayEvent>,
    persist_events: Vec<crate::runtime::dispatch::PersistEvent>,
) -> Vec<Event> {
    if senders.is_some() {
        return Vec::new();
    }

    let mut events = Vec::new();
    for display_event in display_events {
        events.push(Event::Display(display_event));
    }
    for persist_event in persist_events {
        events.push(Event::Persist(persist_event));
    }
    events
}

async fn drain_steering_messages(
    steer_rx: &mut mpsc::UnboundedReceiver<SteerMessage>,
    transcript: &mut TranscriptManager,
    lifecycle_dispatcher: &TaskLifecycleDispatcher,
) -> Vec<Event> {
    let mut events = Vec::new();
    while let Ok(msg) = steer_rx.try_recv() {
        transcript.push_user(msg.message.clone());
        if let Some(ev) = lifecycle_dispatcher
            .steered(msg.source_task_id, msg.source_agent_id, msg.message)
            .await
        {
            events.push(ev);
        }
    }
    events
}

async fn run_step_dispatch(
    deps: &AgentRunDeps,
    ctx: &RunContext,
    transcript: &TranscriptManager,
    spec: &AgentSpec,
    host_context: &Option<crate::domain::tasks::task::HostTaskContext>,
    task_id: &str,
    agent_id: &str,
    model: ModelSpec,
    msg_id: String,
    step_id: String,
    tools: Vec<piko_protocol::tools::ToolDef>,
    senders: Option<&crate::runtime::dispatch::DispatchSenders>,
) -> Result<StepDispatchResult, StepDispatchFailure> {
    let request = GatewayRequest {
        run_id: task_id.to_string(),
        step_id,
        transcript: transcript.to_vec(),
        system_prompt: spec.system_prompt.clone(),
        model: model.id.clone(),
        provider: model.provider.clone(),
        tools,
        thinking: deps
            .model_config
            .as_ref()
            .and_then(|c| c.resolve_thinking()),
    };

    match deps
        .model_executor
        .chat_stream(request, Some(ctx.cancel.clone()))
        .await
    {
        Ok(llm) => {
            let mut dispatch = StepDispatch::from_step_stream(
                host_context
                    .as_ref()
                    .map(|hc| hc.session_id.clone())
                    .unwrap_or_default(),
                task_id.to_string(),
                agent_id.to_string(),
                msg_id,
                model,
                llm,
            );
            let result = dispatch.dispatch_step(senders).await;
            drop(dispatch);
            Ok(result)
        }
        Err(error) => {
            let mut dispatch = StepDispatch::from_step_failure(
                host_context
                    .as_ref()
                    .map(|hc| hc.session_id.clone())
                    .unwrap_or_default(),
                task_id.to_string(),
                agent_id.to_string(),
                msg_id,
                model,
                error.to_string(),
            );
            let result = dispatch.dispatch_step(senders).await;
            drop(dispatch);
            Err(StepDispatchFailure {
                error: error.to_string(),
                result,
            })
        }
    }
}

struct StepDispatchFailure {
    error: String,
    result: StepDispatchResult,
}

async fn wait_for_next_turn_input(
    ctx: &RunContext,
    steer_rx: &mut mpsc::UnboundedReceiver<SteerMessage>,
) -> Option<SteerMessage> {
    tokio::select! {
        _ = ctx.cancel.cancelled() => None,
        msg = steer_rx.recv() => msg,
    }
}

// ---- Entry point: unified agent loop ----

#[allow(unused_assignments)]
pub(crate) fn agent_loop(
    ctx: RunContext,
    steer_rx: mpsc::UnboundedReceiver<SteerMessage>,
    deps: AgentRunDeps,
    task: AgentTask,
    spec: AgentSpec,
    spawner: Arc<dyn AgentSpawner>,
    senders: Option<crate::runtime::dispatch::DispatchSenders>,
) -> impl Stream<Item = Event> {
    stream! {
        let mut orchestrator = TaskOrchestrator::new(
            ctx,
            steer_rx,
            deps,
            task,
            spec,
            spawner,
            senders,
        );
        for event in orchestrator.initialize_events().await {
            yield event;
        }

        'agent: loop {
            // ── Cancel check (top of loop) ──
            if orchestrator.ctx.cancel.is_cancelled() {
                if let Some(ev) = orchestrator.cancelled_event().await { yield ev; }
                break 'agent;
            }

            // ── Drain steering messages ──
            for event in orchestrator.drain_pending_steers().await {
                yield event;
            }

            // ── Cancel check (after discover) ──
            if orchestrator.ctx.cancel.is_cancelled() {
                if let Some(ev) = orchestrator.cancelled_event().await { yield ev; }
                break 'agent;
            }

            let step_cycle = match orchestrator.run_step_cycle().await {
                Ok(cycle) => cycle,
                Err(failure) => {
                    for event in orchestrator.handle_step_failure(failure).await {
                        yield event;
                    }
                    break 'agent;
                }
            };

            // ── Cancel check (after tool calls built) ──
            if orchestrator.ctx.cancel.is_cancelled() {
                if let Some(ev) = orchestrator.cancelled_event().await { yield ev; }
                break 'agent;
            }

            match orchestrator.advance_after_step(step_cycle).await {
                StepAdvance::AwaitNextTurn { events, summary } => {
                    for event in events {
                        yield event;
                    }
                    let (next_events, should_continue) = orchestrator.wait_for_next_turn(summary).await;
                    for event in next_events {
                        yield event;
                    }
                    if should_continue {
                        continue 'agent;
                    }
                    break 'agent;
                }
                StepAdvance::ExecuteTools { events, pending } => {
                    for event in events {
                        yield event;
                    }
                    match orchestrator
                        .execute_tool_calls(
                            &pending.tool_calls,
                            &pending.routes,
                            pending.message_id,
                        )
                        .await
                    {
                        Ok(result) => {
                            let super::tool_executor::ToolExecutionResult { events, .. } = result;
                            for ev in events {
                                yield ev;
                            }
                        }
                        Err(error) => {
                            if let Some(ev) = orchestrator.lifecycle_dispatcher.failed(error).await {
                                yield ev;
                            }
                            break 'agent;
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Produce a stable runtime assistant message ID.
pub fn runtime_assistant_message_id(run_id: &str, step_id: &str) -> String {
    format!("{run_id}:{step_id}:assistant")
}

pub fn runtime_tool_call_message_id(parent_message_id: &str, tool_call_index: u32) -> String {
    format!("{parent_message_id}:tool_call:{tool_call_index}")
}
