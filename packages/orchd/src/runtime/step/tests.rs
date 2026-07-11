use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use llmd::gateway::GatewayEvent;
use tokio_stream::iter;

use crate::domain::model::step::ModelSpec;
use piko_protocol::{
    ContentBlock, DisplayEvent, LifecycleEvent, Message, PersistEvent, TaskEvent, TurnEvent,
};

use super::StepDispatch;
use crate::runtime::events::identity::{AgentDispatchContext, DispatchIdentity, StepEventConsumer};

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
    let mut dispatch = StepDispatch::from_step_stream(
        DispatchIdentity::new("session_1".into(), "task_1".into(), "main".into()),
        "assistant_1".into(),
        "work_1".into(),
        model,
        Box::pin(events),
    );

    let result = dispatch.dispatch_step(None).await;

    assert!(result.local_output.display.iter().any(|event| matches!(
        event,
        DisplayEvent::TextDelta { delta, .. } if delta == "hello"
    )));
    assert!(result.local_output.display.iter().any(|event| matches!(
        event,
        DisplayEvent::ThinkingDelta { delta, .. } if delta == "thinking"
    )));
    assert!(result.local_output.display.iter().any(|event| matches!(
        event,
        DisplayEvent::ToolCallDelta { tool_call_id, delta, .. }
            if tool_call_id == "call_1" && delta == "{\"path\""
    )));
    assert_eq!(result.local_output.persist.len(), 2);
    assert!(matches!(
        &result.local_output.persist[0],
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
        &result.local_output.persist[1],
        PersistEvent::ToolCallCommitted { message, .. }
            if matches!(message, Message::ToolCall { id, .. } if id == "call_1")
    ));
}

#[tokio::test]
async fn local_step_output_keeps_finalize_and_tool_commit_order() {
    let events = iter(vec![
        GatewayEvent::ContentDelta("hello".into()),
        GatewayEvent::ToolCallChunk {
            id: "call_1".into(),
            name: "read".into(),
            args_delta: "{}".into(),
        },
        GatewayEvent::Done("tool_use".into()),
    ]);
    let model = ModelSpec {
        id: "gpt-test".into(),
        name: "GPT Test".into(),
        provider: "openai".into(),
    };
    let mut dispatch = StepDispatch::from_step_stream(
        DispatchIdentity::new("session_1".into(), "task_1".into(), "main".into()),
        "assistant_1".into(),
        "work_1".into(),
        model,
        Box::pin(events),
    );

    let result = dispatch.dispatch_step(None).await;

    assert!(matches!(
        result.local_output.display.as_slice(),
        [
            ..,
            DisplayEvent::MessageEnd { .. },
            DisplayEvent::Finalized { .. }
        ]
    ));
    assert!(matches!(
        result.local_output.persist.as_slice(),
        [
            PersistEvent::Finalized { .. },
            PersistEvent::ToolCallCommitted { .. }
        ]
    ));
    assert_eq!(result.step.tool_calls.len(), 1);
}

#[tokio::test]
async fn lifecycle_events_map_to_task_persist_facts() {
    let task_event = TaskEvent::Created {
        session_id: "session_1".into(),
        task_id: "task_1".into(),
        agent_id: "main".into(),
        parent_task_id: None,
        source_agent_id: None,
        prompt: "hello".into(),
        turn_id: "turn_1".into(),
        timestamp: 1,
    };
    let turn_event = TurnEvent::Started {
        session_id: "session_1".into(),
        turn_id: "turn_1".into(),
        root_task_id: "task_1".into(),
        timestamp: 2,
    };

    let lifecycle_events = [
        LifecycleEvent::Task(task_event.clone()),
        LifecycleEvent::Turn(turn_event),
    ];
    let persist_events: Vec<PersistEvent> = lifecycle_events
        .iter()
        .filter_map(|event| match event {
            LifecycleEvent::Task(task_event) => {
                Some(PersistEvent::TaskEventCommitted(task_event.clone()))
            }
            LifecycleEvent::Turn(_) => None,
        })
        .collect();

    assert!(matches!(
        persist_events.first(),
        Some(PersistEvent::TaskEventCommitted(TaskEvent::Created { task_id, .. }))
            if task_id == "task_1"
    ));
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
        "work_1".into(),
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
