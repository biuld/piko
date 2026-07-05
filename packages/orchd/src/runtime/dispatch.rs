use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt;
use llmd::gateway::GatewayEvent;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

use crate::domain::ModelSpec;
use crate::runtime::chunks::LlmChunks;
use crate::runtime::stream::now_ms;

#[cfg(test)]
use piko_protocol::ContentBlock;
#[cfg(test)]
use piko_protocol::{TaskEvent, TurnEvent};
#[allow(unused_imports)]
use piko_protocol::{
    AgentId, Message, MessageId, ServerMessage, SessionId, TaskId,
};

// Import and re-export protocol types used by hostd
pub use piko_protocol::{DisplayEvent, LifecycleEvent, PersistEvent};

// ---- Channel bus: shared channel senders for child agents ----

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub persist_buffer: usize,
    pub display_buffer: usize,
    pub lifecycle_buffer: usize,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            persist_buffer: 64,
            display_buffer: 256,
            lifecycle_buffer: 64,
        }
    }
}

// ---- Dispatch trait ----

#[async_trait]
pub trait Dispatch: Send {
    fn name(&self) -> &str;

    async fn run(
        &mut self,
        persist_tx: mpsc::Sender<Arc<PersistEvent>>,
        display_tx: mpsc::Sender<Arc<DisplayEvent>>,
        lifecycle_tx: Option<mpsc::Sender<Arc<LifecycleEvent>>>,
    );
}

pub struct SessionChannels {
    persist_tx: mpsc::Sender<Arc<PersistEvent>>,
    persist_rx: Option<mpsc::Receiver<Arc<PersistEvent>>>,
    display_tx: mpsc::Sender<Arc<DisplayEvent>>,
    display_rx: Option<mpsc::Receiver<Arc<DisplayEvent>>>,
    lifecycle_tx: mpsc::Sender<Arc<LifecycleEvent>>,
    lifecycle_rx: Option<mpsc::Receiver<Arc<LifecycleEvent>>>,
}

#[derive(Clone, Debug)]
pub struct DispatchSenders {
    pub persist: mpsc::Sender<Arc<PersistEvent>>,
    pub display: mpsc::Sender<Arc<DisplayEvent>>,
    pub lifecycle: mpsc::Sender<Arc<LifecycleEvent>>,
}

impl SessionChannels {
    pub fn new(config: ChannelConfig) -> Self {
        let (persist_tx, persist_rx) = mpsc::channel(config.persist_buffer);
        let (display_tx, display_rx) = mpsc::channel(config.display_buffer);
        let (lifecycle_tx, lifecycle_rx) = mpsc::channel(config.lifecycle_buffer);
        Self {
            persist_tx,
            persist_rx: Some(persist_rx),
            display_tx,
            display_rx: Some(display_rx),
            lifecycle_tx,
            lifecycle_rx: Some(lifecycle_rx),
        }
    }

    pub fn spawn_dispatch<D>(&self, mut dispatch: D, _session_id: SessionId) -> JoinHandle<()>
    where
        D: Dispatch + 'static,
    {
        let persist_tx = self.persist_tx.clone();
        let display_tx = self.display_tx.clone();
        let lifecycle_tx = self.lifecycle_tx.clone();
        tokio::spawn(async move {
            dispatch
                .run(persist_tx, display_tx, Some(lifecycle_tx))
                .await;
        })
    }

    pub fn persist_stream(&mut self) -> Option<ReceiverStream<Arc<PersistEvent>>> {
        self.persist_rx.take().map(ReceiverStream::new)
    }

    pub fn display_stream(&mut self) -> Option<ReceiverStream<Arc<DisplayEvent>>> {
        self.display_rx.take().map(ReceiverStream::new)
    }

    pub fn lifecycle_stream(&mut self) -> Option<ReceiverStream<Arc<LifecycleEvent>>> {
        self.lifecycle_rx.take().map(ReceiverStream::new)
    }

    pub fn persist_sender(&self) -> mpsc::Sender<Arc<PersistEvent>> {
        self.persist_tx.clone()
    }

    pub fn display_sender(&self) -> mpsc::Sender<Arc<DisplayEvent>> {
        self.display_tx.clone()
    }

    pub fn lifecycle_sender(&self) -> mpsc::Sender<Arc<LifecycleEvent>> {
        self.lifecycle_tx.clone()
    }

    pub fn senders(&self) -> DispatchSenders {
        DispatchSenders {
            persist: self.persist_tx.clone(),
            display: self.display_tx.clone(),
            lifecycle: self.lifecycle_tx.clone(),
        }
    }
}

pub struct AgentDispatch {
    name: String,
    source: AgentDispatchSource,
}

enum AgentDispatchSource {
    GatewayEvents(GatewayDispatchInput),
}

struct GatewayDispatchInput {
    session_id: SessionId,
    task_id: TaskId,
    agent_id: AgentId,
    message_id: MessageId,
    model: ModelSpec,
    events: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
}

pub struct LifecycleDispatch {
    name: String,
    events: mpsc::UnboundedReceiver<LifecycleEvent>,
}

impl LifecycleDispatch {
    pub fn new(
        session_id: impl Into<String>,
        events: mpsc::UnboundedReceiver<LifecycleEvent>,
    ) -> Self {
        Self {
            name: format!("lifecycle:{}", session_id.into()),
            events,
        }
    }
}

#[async_trait]
impl Dispatch for LifecycleDispatch {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(
        &mut self,
        persist_tx: mpsc::Sender<Arc<PersistEvent>>,
        _display_tx: mpsc::Sender<Arc<DisplayEvent>>,
        lifecycle_tx: Option<mpsc::Sender<Arc<LifecycleEvent>>>,
    ) {
        let Some(lifecycle_tx) = lifecycle_tx else {
            return;
        };
        while let Some(event) = self.events.recv().await {
            match &event {
                LifecycleEvent::Task(task_event) => {
                    let task_event = Arc::new(task_event.clone());
                    let _ = lifecycle_tx
                        .send(Arc::new(LifecycleEvent::Task((*task_event).clone())))
                        .await;
                    let _ = persist_tx
                        .send(Arc::new(PersistEvent::TaskLifecycle((*task_event).clone())))
                        .await;
                }
                LifecycleEvent::Turn(_) => {
                    let _ = lifecycle_tx.send(Arc::new(event)).await;
                }
            }
        }
    }
}

impl AgentDispatch {
    pub fn from_gateway_events(
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        model: ModelSpec,
        events: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
    ) -> Self {
        Self {
            name: format!("agent:{agent_id}"),
            source: AgentDispatchSource::GatewayEvents(GatewayDispatchInput {
                session_id,
                task_id,
                agent_id,
                message_id,
                model,
                events,
            }),
        }
    }
}

#[async_trait]
impl Dispatch for AgentDispatch {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(
        &mut self,
        persist_tx: mpsc::Sender<Arc<PersistEvent>>,
        display_tx: mpsc::Sender<Arc<DisplayEvent>>,
        lifecycle_tx: Option<mpsc::Sender<Arc<LifecycleEvent>>>,
    ) {
        let _ = lifecycle_tx;
        let AgentDispatchSource::GatewayEvents(input) = &mut self.source;
        run_gateway_dispatch(input, persist_tx, display_tx).await;
    }
}

async fn run_gateway_dispatch(
    input: &mut GatewayDispatchInput,
    persist_tx: mpsc::Sender<Arc<PersistEvent>>,
    display_tx: mpsc::Sender<Arc<DisplayEvent>>,
) {
    let _ = display_tx
        .send(Arc::new(DisplayEvent::MessageStart {
            message_id: input.message_id.clone(),
            task_id: input.task_id.clone(),
            agent_id: input.agent_id.clone(),
            role: piko_protocol::MessageRole::Assistant,
        }))
        .await;

    let mut chunks = LlmChunks::new();
    while let Some(event) = input.events.next().await {
        match event {
            GatewayEvent::ContentDelta(delta) => {
                chunks.text.push_str(&delta);
                let _ = display_tx
                    .send(Arc::new(DisplayEvent::TextDelta {
                        message_id: input.message_id.clone(),
                        task_id: input.task_id.clone(),
                        agent_id: input.agent_id.clone(),
                        content_index: chunks.text.len() as u32,
                        delta,
                    }))
                    .await;
            }
            GatewayEvent::ReasoningDelta(delta) => {
                chunks.reasoning.push_str(&delta);
                let _ = display_tx
                    .send(Arc::new(DisplayEvent::ThinkingDelta {
                        message_id: input.message_id.clone(),
                        task_id: input.task_id.clone(),
                        agent_id: input.agent_id.clone(),
                        content_index: chunks.reasoning.len() as u32,
                        delta,
                    }))
                    .await;
            }
            other => {
                let done = matches!(other, GatewayEvent::Done(_));
                chunks.apply_non_delta(other);
                if done {
                    break;
                }
            }
        }
    }

    let _ = display_tx
        .send(Arc::new(DisplayEvent::MessageEnd {
            message_id: input.message_id.clone(),
            task_id: input.task_id.clone(),
            agent_id: input.agent_id.clone(),
            stop_reason: Some(chunks.stop_reason.clone()),
            error_message: chunks.error_message.clone(),
        }))
        .await;

    let assistant_message = chunks.build_message(&input.model);
    let finalized_display = Arc::new(DisplayEvent::Finalized {
        message_id: input.message_id.clone(),
        task_id: input.task_id.clone(),
        agent_id: input.agent_id.clone(),
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
    let finalized_persist = Arc::new(PersistEvent::Finalized {
        session_id: input.session_id.clone(),
        message_id: input.message_id.clone(),
        task_id: input.task_id.clone(),
        agent_id: input.agent_id.clone(),
        message: assistant_message,
    });
    let _ = display_tx.send(finalized_display).await;
    let _ = persist_tx.send(finalized_persist).await;

    for tool_call in chunks.take_tool_calls() {
        let message_id = format!(
            "{}:tool_call:{}",
            input.message_id, tool_call.tool_call_index
        );
        let message = Message::ToolCall {
            id: tool_call.id,
            name: tool_call.name,
            arguments: tool_call.arguments,
            model: Some(input.model.id.clone()),
            provider: Some(input.model.provider.clone()),
            timestamp: Some(now_ms()),
        };
        let persist_event = Arc::new(PersistEvent::ToolCallCommitted {
            session_id: input.session_id.clone(),
            message_id,
            task_id: input.task_id.clone(),
            agent_id: input.agent_id.clone(),
            parent_message_id: input.message_id.clone(),
            message,
        });
        let _ = persist_tx.send(persist_event).await;
    }
}

pub fn persist_events_from_server_message(event: &ServerMessage) -> Vec<Arc<PersistEvent>> {
    match event {
        ServerMessage::TaskLifecycle(task_event) => {
            vec![Arc::new(PersistEvent::TaskLifecycle(task_event.clone()))]
        }
        _ => Vec::new(),
    }
}

pub fn display_events_from_server_message(event: &ServerMessage) -> Vec<Arc<DisplayEvent>> {
    match event {
        ServerMessage::Display(display) => vec![Arc::new(display.clone())],
        _ => Vec::new(),
    }
}

pub fn lifecycle_events_from_server_message(event: &ServerMessage) -> Vec<Arc<LifecycleEvent>> {
    match event {
        ServerMessage::TaskLifecycle(event) => vec![Arc::new(LifecycleEvent::Task(event.clone()))],
        ServerMessage::TurnLifecycle(event) => vec![Arc::new(LifecycleEvent::Turn(event.clone()))],
        _ => Vec::new(),
    }
}

pub fn server_message_from_persist_event(event: &PersistEvent) -> Option<ServerMessage> {
    match event {
        PersistEvent::TaskLifecycle(event) => Some(ServerMessage::TaskLifecycle(event.clone())),
        _ => None,
    }
}

pub fn server_message_from_display_event(event: &DisplayEvent) -> Option<ServerMessage> {
    match event {
        DisplayEvent::ToolCallDelta { .. } => None,
        _ => Some(ServerMessage::Display(event.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;
    use tokio_stream::iter;

    struct OneShotDispatch;

    #[async_trait]
    impl Dispatch for OneShotDispatch {
        fn name(&self) -> &str {
            "one-shot"
        }

        async fn run(
            &mut self,
            persist_tx: mpsc::Sender<Arc<PersistEvent>>,
            display_tx: mpsc::Sender<Arc<DisplayEvent>>,
            _lifecycle_tx: Option<mpsc::Sender<Arc<LifecycleEvent>>>,
        ) {
            let display = Arc::new(DisplayEvent::TextDelta {
                message_id: "m1".into(),
                task_id: "t1".into(),
                agent_id: "a1".into(),
                content_index: 0,
                delta: "hello".into(),
            });
            display_tx.send(display).await.unwrap();

            let persist = Arc::new(PersistEvent::TaskLifecycle(TaskEvent::Started {
                session_id: "s1".into(),
                task_id: "t1".into(),
                agent_id: "a1".into(),
                timestamp: 1,
            }));
            persist_tx.send(persist).await.unwrap();
        }
    }

    #[tokio::test]
    async fn session_channels_fan_out_dispatch_output_by_type() {
        let mut channels = SessionChannels::new(ChannelConfig::default());
        let mut persist = channels.persist_stream().unwrap();
        let mut display = channels.display_stream().unwrap();

        let handle = channels.spawn_dispatch(OneShotDispatch, "s1".into());
        handle.await.unwrap();
        drop(channels);

        assert!(matches!(
            display.next().await.as_deref(),
            Some(DisplayEvent::TextDelta { delta, .. }) if delta == "hello"
        ));
        assert!(matches!(
            persist.next().await.as_deref(),
            Some(PersistEvent::TaskLifecycle(TaskEvent::Started { task_id, .. })) if task_id == "t1"
        ));
    }

    #[tokio::test]
    async fn agent_dispatch_routes_gateway_events_without_persisting_deltas() {
        let events = iter(vec![
            GatewayEvent::ContentDelta("hello".into()),
            GatewayEvent::ReasoningDelta("thinking".into()),
            GatewayEvent::ToolCallChunk {
                id: "call_1".into(),
                name: "read".into(),
                args_delta: "{\"path\"".into(),
            },
            GatewayEvent::ToolCallChunk {
                id: "ignored-continuation-id".into(),
                name: String::new(),
                args_delta: ":\"Cargo.toml\"}".into(),
            },
            GatewayEvent::Done("tool_use".into()),
        ]);
        let model = ModelSpec {
            id: "gpt-test".into(),
            name: "GPT Test".into(),
            provider: "openai".into(),
        };
        let mut channels = SessionChannels::new(ChannelConfig::default());
        let persist = channels.persist_stream().unwrap();
        let display = channels.display_stream().unwrap();

        let handle = channels.spawn_dispatch(
            AgentDispatch::from_gateway_events(
                "session_1".into(),
                "task_1".into(),
                "main".into(),
                "assistant_1".into(),
                model,
                Box::pin(events),
            ),
            "session_1".into(),
        );
        handle.await.unwrap();
        drop(channels);

        let display_events: Vec<_> = display.collect().await;
        let persist_events: Vec<_> = persist.collect().await;

        assert!(display_events.iter().any(|event| matches!(
            event.as_ref(),
            DisplayEvent::TextDelta { delta, .. } if delta == "hello"
        )));
        assert!(display_events.iter().any(|event| matches!(
            event.as_ref(),
            DisplayEvent::ThinkingDelta { delta, .. } if delta == "thinking"
        )));
        assert_eq!(persist_events.len(), 2);
        assert!(matches!(
            persist_events[0].as_ref(),
            PersistEvent::Finalized { message, .. }
                if matches!(message, Message::Assistant { content, stop_reason, .. }
                    if stop_reason == &Some("tool_use".into())
                        && content.iter().any(|block| matches!(
                            block,
                            ContentBlock::Text { text } if text == "hello"
                        ))
                    )
        ));
        assert!(matches!(
            persist_events[1].as_ref(),
            PersistEvent::ToolCallCommitted { message, .. }
                if matches!(message, Message::ToolCall { id, .. } if id == "call_1")
        ));
    }

    #[tokio::test]
    async fn lifecycle_dispatch_routes_task_and_turn_lifecycle() {
        let (tx, rx) = mpsc::unbounded_channel();
        tx.send(LifecycleEvent::Task(TaskEvent::Created {
            session_id: "session_1".into(),
            task_id: "task_1".into(),
            agent_id: "main".into(),
            parent_task_id: None,
            source_agent_id: None,
            prompt: "hello".into(),
            turn_id: "turn_1".into(),
            timestamp: 1,
        }))
        .unwrap();
        tx.send(LifecycleEvent::Turn(TurnEvent::Started {
            session_id: "session_1".into(),
            turn_id: "turn_1".into(),
            root_task_id: "task_1".into(),
            timestamp: 2,
        }))
        .unwrap();
        drop(tx);

        let mut channels = SessionChannels::new(ChannelConfig::default());
        let mut persist = channels.persist_stream().unwrap();
        let mut lifecycle = channels.lifecycle_stream().unwrap();
        let handle =
            channels.spawn_dispatch(LifecycleDispatch::new("session_1", rx), "session_1".into());
        handle.await.unwrap();
        drop(channels);

        assert!(matches!(
            lifecycle.next().await.as_deref(),
            Some(LifecycleEvent::Task(TaskEvent::Created { task_id, .. }))
                if task_id == "task_1"
        ));
        assert!(matches!(
            lifecycle.next().await.as_deref(),
            Some(LifecycleEvent::Turn(TurnEvent::Started { turn_id, .. }))
                if turn_id == "turn_1"
        ));
        assert!(matches!(
            persist.next().await.as_deref(),
            Some(PersistEvent::TaskLifecycle(TaskEvent::Created { task_id, .. }))
                if task_id == "task_1"
        ));
        assert!(persist.next().await.is_none());
    }
}
