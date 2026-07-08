use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt;
use llmd::gateway::GatewayEvent;
use tokio::sync::mpsc;

use crate::domain::ModelSpec;
use crate::domain::model::transcript::{ContentBlock, MessageUsage};
use crate::runtime::stream::now_ms;
use crate::runtime::tool_calls::{ToolCallAggregator, ToolCallItem};
use piko_protocol::{AgentId, Message, MessageId, SessionId, TaskId};

use super::{Dispatch, DispatchSenders, DisplayEvent, PersistEvent};

pub struct StepDispatch {
    name: String,
    source: StepDispatchSource,
    consumers: Vec<Box<dyn AgentEventConsumer>>,
}

enum StepDispatchSource {
    StepStream(StepDispatchInput),
    StepFailure(StepFailureInput),
}

struct StepDispatchInput {
    session_id: SessionId,
    task_id: TaskId,
    agent_id: AgentId,
    message_id: MessageId,
    model: ModelSpec,
    events: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
}

struct StepFailureInput {
    session_id: SessionId,
    task_id: TaskId,
    agent_id: AgentId,
    message_id: MessageId,
    model: ModelSpec,
    error_message: String,
}

impl StepDispatch {
    pub fn from_step_stream(
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        model: ModelSpec,
        events: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
    ) -> Self {
        Self {
            name: format!("agent:{agent_id}"),
            source: StepDispatchSource::StepStream(StepDispatchInput {
                session_id,
                task_id,
                agent_id,
                message_id,
                model,
                events,
            }),
            consumers: Vec::new(),
        }
    }

    pub fn from_step_failure(
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        model: ModelSpec,
        error_message: String,
    ) -> Self {
        Self {
            name: format!("agent:{agent_id}"),
            source: StepDispatchSource::StepFailure(StepFailureInput {
                session_id,
                task_id,
                agent_id,
                message_id,
                model,
                error_message,
            }),
            consumers: Vec::new(),
        }
    }

    pub(crate) fn register_consumer<C>(&mut self, consumer: C) -> &mut Self
    where
        C: AgentEventConsumer + 'static,
    {
        self.consumers.push(Box::new(consumer));
        self
    }

    pub(crate) async fn dispatch_step(
        &mut self,
        senders: Option<&DispatchSenders>,
    ) -> StepDispatchResult {
        let (
            assistant_message_collector,
            persist_collector,
            display_collector,
            tool_call_collector,
        ) = self.configure_step_consumers(senders);
        match &mut self.source {
            StepDispatchSource::StepStream(input) => {
                dispatch_step_stream(
                    input,
                    &mut self.consumers,
                    assistant_message_collector,
                    persist_collector,
                    display_collector,
                    tool_call_collector,
                )
                .await
            }
            StepDispatchSource::StepFailure(input) => {
                dispatch_step_failure(
                    input,
                    &mut self.consumers,
                    assistant_message_collector,
                    persist_collector,
                    display_collector,
                    tool_call_collector,
                )
                .await
            }
        }
    }

    fn configure_step_consumers(
        &mut self,
        senders: Option<&DispatchSenders>,
    ) -> (
        SharedAssistantMessageCollector,
        SharedPersistCollector,
        SharedDisplayCollector,
        SharedToolCallCollector,
    ) {
        let assistant_message_collector = SharedAssistantMessageCollector::default();
        let persist_collector = SharedPersistCollector::default();
        let display_collector = SharedDisplayCollector::default();
        let tool_call_collector = SharedToolCallCollector::default();
        if let Some(senders) = senders {
            self.register_consumer(DisplayChannelConsumer::new(
                senders.display.clone(),
                AssistantMessageState::new(),
            ));
            self.register_consumer(AssistantPersistChannelConsumer::new(
                senders.persist.clone(),
                assistant_message_collector.clone(),
                AssistantMessageState::new(),
            ));
            self.register_consumer(ToolCallChannelConsumer::new(
                senders.display.clone(),
                senders.persist.clone(),
                tool_call_collector.clone(),
            ));
        } else {
            self.register_consumer(DisplayCollectingConsumer::new(
                display_collector.clone(),
                AssistantMessageState::new(),
            ));
            self.register_consumer(AssistantPersistCollectingConsumer::new(
                persist_collector.clone(),
                assistant_message_collector.clone(),
                AssistantMessageState::new(),
            ));
            self.register_consumer(ToolCallCollectingConsumer::new(
                display_collector.clone(),
                persist_collector.clone(),
                tool_call_collector.clone(),
            ));
        }
        (
            assistant_message_collector,
            persist_collector,
            display_collector,
            tool_call_collector,
        )
    }
}

pub(crate) struct AgentDispatchContext<'a> {
    pub session_id: &'a SessionId,
    pub task_id: &'a TaskId,
    pub agent_id: &'a AgentId,
    pub message_id: &'a MessageId,
    pub model: &'a ModelSpec,
}

#[allow(dead_code)]
#[async_trait]
pub(crate) trait AgentEventConsumer: Send {
    async fn on_task_created(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _parent_task_id: Option<&str>,
        _source_agent_id: Option<&str>,
        _prompt: &str,
        _turn_id: &str,
    ) {
    }

    async fn on_task_started(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_task_steered(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _source_task_id: &str,
        _source_agent_id: &str,
        _message: &str,
    ) {
    }

    async fn on_task_idle(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _total_steps: u32,
        _summary: &str,
    ) {
    }

    async fn on_task_failed(&mut self, _ctx: &AgentDispatchContext<'_>, _error: &str) {}

    async fn on_task_completed(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _total_steps: u32,
        _summary: &str,
    ) {
    }

    async fn on_task_cancelled(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_step_started(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, _event: &GatewayEvent) {}

    async fn on_step_finished(&mut self, _ctx: &AgentDispatchContext<'_>) {}

    async fn on_assistant_message_committed(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _message: &Message,
        _tool_calls: &[ToolCallItem],
    ) {
    }

    async fn on_tool_started(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _tool_call: &ToolCallItem,
        _msg_id: &str,
    ) {
    }

    async fn on_tool_ended(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _tool_call: &ToolCallItem,
        _result: &serde_json::Value,
        _is_error: bool,
    ) {
    }

    async fn on_tool_result_committed(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _tool_call_index: u32,
        _message: &Message,
        _msg_id: &str,
    ) {
    }
}

#[derive(Clone)]
struct AssistantMessageState {
    text: String,
    reasoning: String,
    usage: Option<MessageUsage>,
    stop_reason: String,
    error_message: Option<String>,
}

impl AssistantMessageState {
    fn new() -> Self {
        Self {
            text: String::new(),
            reasoning: String::new(),
            usage: None,
            stop_reason: "stop".into(),
            error_message: None,
        }
    }

    fn apply_gateway_event(&mut self, event: &GatewayEvent) {
        match event {
            GatewayEvent::ContentDelta(delta) => self.text.push_str(delta),
            GatewayEvent::ReasoningDelta(delta) => self.reasoning.push_str(delta),
            GatewayEvent::Usage(usage) => self.usage = Some(usage.clone()),
            GatewayEvent::Done(reason) => self.stop_reason = reason.clone(),
            GatewayEvent::Error(error) => {
                tracing::error!("Stream error: {error}");
                self.stop_reason = "error".into();
                self.error_message = Some(error.clone());
            }
            GatewayEvent::ToolCallChunk { .. } => {}
        }
    }

    fn build_message(&self, model: &ModelSpec) -> Message {
        let mut blocks = Vec::new();
        if !self.reasoning.is_empty() {
            blocks.push(ContentBlock::Thinking {
                thinking: self.reasoning.clone(),
                thinking_signature: None,
            });
        }
        if !self.text.is_empty() {
            blocks.push(ContentBlock::Text {
                text: self.text.clone(),
            });
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::Text {
                text: String::new(),
            });
        }
        Message::Assistant {
            content: blocks,
            api: "openai-completions".into(),
            provider: model.provider.clone(),
            model: model.id.clone(),
            usage: self.usage.clone(),
            stop_reason: Some(self.stop_reason.clone()),
            error_message: self.error_message.clone(),
            timestamp: Some(now_ms()),
        }
    }
}

pub(crate) struct StepDispatchResult {
    pub assistant_message: Message,
    pub tool_calls: Vec<ToolCallItem>,
    pub display_events: Vec<DisplayEvent>,
    pub persist_events: Vec<PersistEvent>,
}

#[async_trait]
impl Dispatch for StepDispatch {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(
        &mut self,
        persist_tx: mpsc::Sender<Arc<PersistEvent>>,
        display_tx: mpsc::Sender<Arc<DisplayEvent>>,
        lifecycle_tx: Option<mpsc::Sender<Arc<super::LifecycleEvent>>>,
    ) {
        let _ = lifecycle_tx;
        let assistant_message_collector;
        let persist_collector;
        let display_collector;
        let tool_call_collector;
        {
            let senders = DispatchSenders {
                persist: persist_tx.clone(),
                display: display_tx.clone(),
                lifecycle: mpsc::unbounded_channel().0,
            };
            (
                assistant_message_collector,
                persist_collector,
                display_collector,
                tool_call_collector,
            ) = self.configure_step_consumers(Some(&senders));
        }
        match &mut self.source {
            StepDispatchSource::StepStream(input) => {
                let _ = dispatch_step_stream(
                    input,
                    &mut self.consumers,
                    assistant_message_collector,
                    persist_collector,
                    display_collector,
                    tool_call_collector,
                )
                .await;
            }
            StepDispatchSource::StepFailure(input) => {
                let _ = dispatch_step_failure(
                    input,
                    &mut self.consumers,
                    assistant_message_collector,
                    persist_collector,
                    display_collector,
                    tool_call_collector,
                )
                .await;
            }
        }
    }
}

async fn dispatch_step_stream(
    input: &mut StepDispatchInput,
    consumers: &mut Vec<Box<dyn AgentEventConsumer>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    persist_collector: SharedPersistCollector,
    display_collector: SharedDisplayCollector,
    tool_call_collector: SharedToolCallCollector,
) -> StepDispatchResult {
    let ctx = AgentDispatchContext {
        session_id: &input.session_id,
        task_id: &input.task_id,
        agent_id: &input.agent_id,
        message_id: &input.message_id,
        model: &input.model,
    };

    for consumer in consumers.iter_mut() {
        consumer.on_step_started(&ctx).await;
    }

    while let Some(event) = input.events.next().await {
        for consumer in consumers.iter_mut() {
            consumer.on_gateway_event(&ctx, &event).await;
        }

        if matches!(event, GatewayEvent::Done(_)) {
            break;
        }
    }

    for consumer in consumers.iter_mut() {
        consumer.on_step_finished(&ctx).await;
    }

    let result = StepDispatchResult {
        assistant_message: assistant_message_collector.take(),
        tool_calls: tool_call_collector.take(),
        display_events: display_collector.take(),
        persist_events: persist_collector.take(),
    };

    for consumer in consumers.iter_mut() {
        consumer
            .on_assistant_message_committed(&ctx, &result.assistant_message, &result.tool_calls)
            .await;
    }

    result
}

async fn dispatch_step_failure(
    input: &mut StepFailureInput,
    consumers: &mut Vec<Box<dyn AgentEventConsumer>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    persist_collector: SharedPersistCollector,
    display_collector: SharedDisplayCollector,
    tool_call_collector: SharedToolCallCollector,
) -> StepDispatchResult {
    let ctx = AgentDispatchContext {
        session_id: &input.session_id,
        task_id: &input.task_id,
        agent_id: &input.agent_id,
        message_id: &input.message_id,
        model: &input.model,
    };

    for consumer in consumers.iter_mut() {
        consumer.on_step_started(&ctx).await;
    }

    let error_event = GatewayEvent::Error(input.error_message.clone());
    for consumer in consumers.iter_mut() {
        consumer.on_gateway_event(&ctx, &error_event).await;
    }
    for consumer in consumers.iter_mut() {
        consumer.on_step_finished(&ctx).await;
    }

    let result = StepDispatchResult {
        assistant_message: assistant_message_collector.take(),
        tool_calls: tool_call_collector.take(),
        display_events: display_collector.take(),
        persist_events: persist_collector.take(),
    };

    for consumer in consumers.iter_mut() {
        consumer
            .on_assistant_message_committed(&ctx, &result.assistant_message, &result.tool_calls)
            .await;
    }

    result
}

#[derive(Clone, Default)]
struct SharedAssistantMessageCollector(Arc<std::sync::Mutex<Option<Message>>>);

impl SharedAssistantMessageCollector {
    fn take(&self) -> Message {
        self.0
            .lock()
            .expect("assistant message collector poisoned")
            .take()
            .expect("assistant message missing")
    }

    fn set(&self, message: Message) {
        *self.0.lock().expect("assistant message collector poisoned") = Some(message);
    }
}

#[derive(Clone, Default)]
struct SharedPersistCollector(Arc<std::sync::Mutex<Vec<PersistEvent>>>);

impl SharedPersistCollector {
    fn take(&self) -> Vec<PersistEvent> {
        let mut events = self.0.lock().expect("persist collector poisoned");
        std::mem::take(&mut *events)
    }

    fn push(&self, event: PersistEvent) {
        let mut events = self.0.lock().expect("persist collector poisoned");
        events.push(event);
    }
}

#[derive(Clone, Default)]
struct SharedDisplayCollector(Arc<std::sync::Mutex<Vec<DisplayEvent>>>);

impl SharedDisplayCollector {
    fn take(&self) -> Vec<DisplayEvent> {
        let mut events = self.0.lock().expect("display collector poisoned");
        std::mem::take(&mut *events)
    }

    fn push(&self, event: DisplayEvent) {
        let mut events = self.0.lock().expect("display collector poisoned");
        events.push(event);
    }
}

#[derive(Clone, Default)]
struct SharedToolCallCollector(Arc<std::sync::Mutex<Vec<ToolCallItem>>>);

impl SharedToolCallCollector {
    fn take(&self) -> Vec<ToolCallItem> {
        let mut tool_calls = self.0.lock().expect("tool call collector poisoned");
        std::mem::take(&mut *tool_calls)
    }

    fn push(&self, tool_call: ToolCallItem) {
        let mut tool_calls = self.0.lock().expect("tool call collector poisoned");
        tool_calls.push(tool_call);
    }
}

struct DisplayChannelConsumer {
    tx: mpsc::Sender<Arc<DisplayEvent>>,
    state: AssistantMessageState,
}

impl DisplayChannelConsumer {
    fn new(tx: mpsc::Sender<Arc<DisplayEvent>>, state: AssistantMessageState) -> Self {
        Self { tx, state }
    }
}

#[async_trait]
impl AgentEventConsumer for DisplayChannelConsumer {
    async fn on_step_started(&mut self, ctx: &AgentDispatchContext<'_>) {
        let _ = self
            .tx
            .send(Arc::new(DisplayEvent::MessageStart {
                message_id: ctx.message_id.clone(),
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                role: piko_protocol::MessageRole::Assistant,
            }))
            .await;
    }

    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
        match event {
            GatewayEvent::ContentDelta(delta) => {
                let _ = self
                    .tx
                    .send(Arc::new(DisplayEvent::TextDelta {
                        message_id: ctx.message_id.clone(),
                        task_id: ctx.task_id.clone(),
                        agent_id: ctx.agent_id.clone(),
                        content_index: self.state.text.len() as u32,
                        delta: delta.clone(),
                    }))
                    .await;
            }
            GatewayEvent::ReasoningDelta(delta) => {
                let _ = self
                    .tx
                    .send(Arc::new(DisplayEvent::ThinkingDelta {
                        message_id: ctx.message_id.clone(),
                        task_id: ctx.task_id.clone(),
                        agent_id: ctx.agent_id.clone(),
                        content_index: self.state.reasoning.len() as u32,
                        delta: delta.clone(),
                    }))
                    .await;
            }
            _ => {}
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self.state.build_message(ctx.model);
        let _ = self
            .tx
            .send(Arc::new(DisplayEvent::MessageEnd {
                message_id: ctx.message_id.clone(),
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                stop_reason: match &assistant_message {
                    Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                    _ => None,
                },
                error_message: match &assistant_message {
                    Message::Assistant { error_message, .. } => error_message.clone(),
                    _ => None,
                },
            }))
            .await;
        let _ = self
            .tx
            .send(Arc::new(DisplayEvent::Finalized {
                message_id: ctx.message_id.clone(),
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                content: match &assistant_message {
                    Message::Assistant { content, .. } => content.clone(),
                    _ => Vec::new(),
                },
                usage: match &assistant_message {
                    Message::Assistant { usage, .. } => usage.clone(),
                    _ => None,
                },
                stop_reason: match &assistant_message {
                    Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                    _ => None,
                },
                error_message: match &assistant_message {
                    Message::Assistant { error_message, .. } => error_message.clone(),
                    _ => None,
                },
            }))
            .await;
    }
}

struct DisplayCollectingConsumer {
    collector: SharedDisplayCollector,
    state: AssistantMessageState,
}

impl DisplayCollectingConsumer {
    fn new(collector: SharedDisplayCollector, state: AssistantMessageState) -> Self {
        Self { collector, state }
    }
}

#[async_trait]
impl AgentEventConsumer for DisplayCollectingConsumer {
    async fn on_step_started(&mut self, ctx: &AgentDispatchContext<'_>) {
        self.collector.push(DisplayEvent::MessageStart {
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            role: piko_protocol::MessageRole::Assistant,
        });
    }

    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
        match event {
            GatewayEvent::ContentDelta(delta) => {
                self.collector.push(DisplayEvent::TextDelta {
                    message_id: ctx.message_id.clone(),
                    task_id: ctx.task_id.clone(),
                    agent_id: ctx.agent_id.clone(),
                    content_index: self.state.text.len() as u32,
                    delta: delta.clone(),
                });
            }
            GatewayEvent::ReasoningDelta(delta) => {
                self.collector.push(DisplayEvent::ThinkingDelta {
                    message_id: ctx.message_id.clone(),
                    task_id: ctx.task_id.clone(),
                    agent_id: ctx.agent_id.clone(),
                    content_index: self.state.reasoning.len() as u32,
                    delta: delta.clone(),
                });
            }
            _ => {}
        }
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self.state.build_message(ctx.model);
        self.collector.push(DisplayEvent::MessageEnd {
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            stop_reason: match &assistant_message {
                Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                _ => None,
            },
            error_message: match &assistant_message {
                Message::Assistant { error_message, .. } => error_message.clone(),
                _ => None,
            },
        });
        self.collector.push(DisplayEvent::Finalized {
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            content: match &assistant_message {
                Message::Assistant { content, .. } => content.clone(),
                _ => Vec::new(),
            },
            usage: match &assistant_message {
                Message::Assistant { usage, .. } => usage.clone(),
                _ => None,
            },
            stop_reason: match &assistant_message {
                Message::Assistant { stop_reason, .. } => stop_reason.clone(),
                _ => None,
            },
            error_message: match &assistant_message {
                Message::Assistant { error_message, .. } => error_message.clone(),
                _ => None,
            },
        });
    }
}

struct AssistantPersistChannelConsumer {
    tx: mpsc::Sender<Arc<PersistEvent>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    state: AssistantMessageState,
}

impl AssistantPersistChannelConsumer {
    fn new(
        tx: mpsc::Sender<Arc<PersistEvent>>,
        assistant_message_collector: SharedAssistantMessageCollector,
        state: AssistantMessageState,
    ) -> Self {
        Self {
            tx,
            assistant_message_collector,
            state,
        }
    }
}

#[async_trait]
impl AgentEventConsumer for AssistantPersistChannelConsumer {
    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self.state.build_message(ctx.model);
        self.assistant_message_collector
            .set(assistant_message.clone());
        let _ = self
            .tx
            .send(Arc::new(PersistEvent::Finalized {
                session_id: ctx.session_id.clone(),
                message_id: ctx.message_id.clone(),
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                message: assistant_message.clone(),
            }))
            .await;
    }
}

struct AssistantPersistCollectingConsumer {
    collector: SharedPersistCollector,
    assistant_message_collector: SharedAssistantMessageCollector,
    state: AssistantMessageState,
}

impl AssistantPersistCollectingConsumer {
    fn new(
        collector: SharedPersistCollector,
        assistant_message_collector: SharedAssistantMessageCollector,
        state: AssistantMessageState,
    ) -> Self {
        Self {
            collector,
            assistant_message_collector,
            state,
        }
    }
}

#[async_trait]
impl AgentEventConsumer for AssistantPersistCollectingConsumer {
    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        self.state.apply_gateway_event(event);
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        let assistant_message = self.state.build_message(ctx.model);
        self.assistant_message_collector
            .set(assistant_message.clone());
        self.collector.push(PersistEvent::Finalized {
            session_id: ctx.session_id.clone(),
            message_id: ctx.message_id.clone(),
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            message: assistant_message.clone(),
        });
    }
}

struct ToolCallChannelConsumer {
    display_tx: mpsc::Sender<Arc<DisplayEvent>>,
    persist_tx: mpsc::Sender<Arc<PersistEvent>>,
    collector: SharedToolCallCollector,
    aggregator: ToolCallAggregator,
}

impl ToolCallChannelConsumer {
    fn new(
        display_tx: mpsc::Sender<Arc<DisplayEvent>>,
        persist_tx: mpsc::Sender<Arc<PersistEvent>>,
        collector: SharedToolCallCollector,
    ) -> Self {
        Self {
            display_tx,
            persist_tx,
            collector,
            aggregator: ToolCallAggregator::new(),
        }
    }
}

#[async_trait]
impl AgentEventConsumer for ToolCallChannelConsumer {
    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        let Some(update) = self.aggregator.on_gateway_event(event) else {
            return;
        };
        let _ = self
            .display_tx
            .send(Arc::new(DisplayEvent::ToolCallDelta {
                task_id: ctx.task_id.clone(),
                agent_id: ctx.agent_id.clone(),
                message_id: ctx.message_id.clone(),
                content_index: update.content_index,
                tool_call_id: update.tool_call_id,
                delta: update.delta,
            }))
            .await;
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        for tool_call in self.aggregator.flush() {
            let message_id = format!("{}:tool_call:{}", ctx.message_id, tool_call.tool_call_index);
            let message = Message::ToolCall {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
                model: Some(ctx.model.id.clone()),
                provider: Some(ctx.model.provider.clone()),
                timestamp: Some(now_ms()),
            };
            let _ = self
                .persist_tx
                .send(Arc::new(PersistEvent::ToolCallCommitted {
                    session_id: ctx.session_id.clone(),
                    message_id,
                    task_id: ctx.task_id.clone(),
                    agent_id: ctx.agent_id.clone(),
                    parent_message_id: ctx.message_id.clone(),
                    message,
                }))
                .await;
            self.collector.push(tool_call);
        }
    }
}

struct ToolCallCollectingConsumer {
    display_collector: SharedDisplayCollector,
    persist_collector: SharedPersistCollector,
    tool_call_collector: SharedToolCallCollector,
    aggregator: ToolCallAggregator,
}

impl ToolCallCollectingConsumer {
    fn new(
        display_collector: SharedDisplayCollector,
        persist_collector: SharedPersistCollector,
        tool_call_collector: SharedToolCallCollector,
    ) -> Self {
        Self {
            display_collector,
            persist_collector,
            tool_call_collector,
            aggregator: ToolCallAggregator::new(),
        }
    }
}

#[async_trait]
impl AgentEventConsumer for ToolCallCollectingConsumer {
    async fn on_gateway_event(&mut self, ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        let Some(update) = self.aggregator.on_gateway_event(event) else {
            return;
        };
        self.display_collector.push(DisplayEvent::ToolCallDelta {
            task_id: ctx.task_id.clone(),
            agent_id: ctx.agent_id.clone(),
            message_id: ctx.message_id.clone(),
            content_index: update.content_index,
            tool_call_id: update.tool_call_id,
            delta: update.delta,
        });
    }

    async fn on_step_finished(&mut self, ctx: &AgentDispatchContext<'_>) {
        for tool_call in self.aggregator.flush() {
            let message_id = format!("{}:tool_call:{}", ctx.message_id, tool_call.tool_call_index);
            let message = Message::ToolCall {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
                model: Some(ctx.model.id.clone()),
                provider: Some(ctx.model.provider.clone()),
                timestamp: Some(now_ms()),
            };
            self.persist_collector
                .push(PersistEvent::ToolCallCommitted {
                    session_id: ctx.session_id.clone(),
                    message_id,
                    task_id: ctx.task_id.clone(),
                    agent_id: ctx.agent_id.clone(),
                    parent_message_id: ctx.message_id.clone(),
                    message,
                });
            self.tool_call_collector.push(tool_call);
        }
    }
}
