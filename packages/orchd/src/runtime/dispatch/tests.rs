use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use llmd::gateway::GatewayEvent;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::iter;

use crate::domain::ModelSpec;
use piko_protocol::{ContentBlock, Message, TaskEvent, TurnEvent};

use super::consumer::{AgentDispatchContext, DispatchIdentity, StepEventConsumer};
use super::{
    ChannelConfig, Dispatch, DisplayEvent, LifecycleDispatch, LifecycleEvent, PersistEvent,
    SessionChannels, StepDispatch,
};

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

        let persist = Arc::new(PersistEvent::TaskEventCommitted(TaskEvent::Started {
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
        Some(PersistEvent::TaskEventCommitted(TaskEvent::Started { task_id, .. })) if task_id == "t1"
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
        StepDispatch::from_step_stream(
            DispatchIdentity::new("session_1".into(), "task_1".into(), "main".into()),
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
    assert!(display_events.iter().any(|event| matches!(
        event.as_ref(),
        DisplayEvent::ToolCallDelta { tool_call_id, delta, .. }
            if tool_call_id == "call_1" && delta == "{\"path\""
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
        Some(PersistEvent::TaskEventCommitted(TaskEvent::Created { task_id, .. }))
            if task_id == "task_1"
    ));
    assert!(persist.next().await.is_none());
}

#[derive(Clone, Default)]
struct SeenEvents(Arc<Mutex<Vec<&'static str>>>);

impl SeenEvents {
    fn push(&self, value: &'static str) {
        self.0.lock().unwrap().push(value);
    }

    fn values(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().clone()
    }
}

struct RecordingConsumer {
    seen: SeenEvents,
}

#[async_trait]
impl StepEventConsumer for RecordingConsumer {
    async fn on_gateway_event(&mut self, _ctx: &AgentDispatchContext<'_>, event: &GatewayEvent) {
        match event {
            GatewayEvent::ContentDelta(_) => self.seen.push("content"),
            GatewayEvent::ReasoningDelta(_) => self.seen.push("reasoning"),
            GatewayEvent::Done(_) => self.seen.push("done"),
            _ => self.seen.push("other"),
        }
    }

    async fn on_step_finished(&mut self, _ctx: &AgentDispatchContext<'_>) {
        self.seen.push("finished");
    }

    async fn on_assistant_message_committed(
        &mut self,
        _ctx: &AgentDispatchContext<'_>,
        _message: &Message,
        _tool_calls: &[crate::runtime::types::ToolCallItem],
    ) {
        self.seen.push("committed");
    }
}

#[tokio::test]
async fn agent_dispatch_invokes_registered_consumers() {
    let events = iter(vec![
        GatewayEvent::ContentDelta("hello".into()),
        GatewayEvent::ReasoningDelta("thinking".into()),
        GatewayEvent::Done("stop".into()),
    ]);
    let seen = SeenEvents::default();
    let model = ModelSpec {
        id: "gpt-test".into(),
        name: "GPT Test".into(),
        provider: "openai".into(),
    };

    let mut dispatch = StepDispatch::from_step_stream(
        DispatchIdentity::new("session_1".into(), "task_1".into(), "main".into()),
        "assistant_1".into(),
        model,
        Box::pin(events),
    );
    dispatch.register_consumer(RecordingConsumer { seen: seen.clone() });

    let result = dispatch.dispatch_step(None).await;

    assert!(matches!(
        result.local_output.persist.first(),
        Some(PersistEvent::Finalized { .. })
    ));
    assert_eq!(
        seen.values(),
        vec!["content", "reasoning", "done", "finished", "committed"]
    );
}
