use futures_util::StreamExt;
use llmd::gateway::GatewayEvent;

use super::StepDispatchResult;
use super::collectors::{
    SharedAssistantMessageCollector, SharedDisplayCollector, SharedPersistCollector,
};
use super::source::{StepDispatchInput, StepFailureInput};
use crate::runtime::dispatch::consumer::{AgentDispatchContext, AgentEventConsumer};

pub(crate) async fn dispatch_step_stream(
    input: &mut StepDispatchInput,
    consumers: &mut Vec<Box<dyn AgentEventConsumer>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    persist_collector: SharedPersistCollector,
    display_collector: SharedDisplayCollector,
    tool_call_collector: crate::runtime::dispatch::consumer::tool::SharedToolCallCollector,
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

pub(crate) async fn dispatch_step_failure(
    input: &mut StepFailureInput,
    consumers: &mut Vec<Box<dyn AgentEventConsumer>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    persist_collector: SharedPersistCollector,
    display_collector: SharedDisplayCollector,
    tool_call_collector: crate::runtime::dispatch::consumer::tool::SharedToolCallCollector,
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
