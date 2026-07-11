use futures_util::StreamExt;
use llmd::gateway::GatewayEvent;

use super::collectors::{
    SharedAssistantMessageCollector, SharedDisplayCollector, SharedPersistCollector,
};
use super::source::{StepDispatchInput, StepFailureInput};
use super::{CompletedStep, LocalStepOutput, StepDispatchResult};
use crate::runtime::dispatch::consumer::StepEventConsumer;

pub(crate) async fn dispatch_step_stream(
    input: &mut StepDispatchInput,
    consumers: &mut Vec<Box<dyn StepEventConsumer>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    persist_collector: SharedPersistCollector,
    display_collector: SharedDisplayCollector,
    tool_call_collector: crate::runtime::dispatch::consumer::tool::SharedToolCallCollector,
) -> StepDispatchResult {
    let ctx = input
        .identity
        .as_context(&input.message_id, Some(&input.model), &input.work_id);

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

    let assistant_message = assistant_message_collector.take();
    let tool_calls = tool_call_collector.take();

    for consumer in consumers.iter_mut() {
        consumer
            .on_assistant_message_committed(&ctx, &assistant_message, &tool_calls)
            .await;
    }

    StepDispatchResult {
        step: CompletedStep {
            assistant_message,
            tool_calls,
        },
        local_output: LocalStepOutput {
            display: display_collector.take(),
            persist: persist_collector.take(),
        },
    }
}

pub(crate) async fn dispatch_step_failure(
    input: &mut StepFailureInput,
    consumers: &mut Vec<Box<dyn StepEventConsumer>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    persist_collector: SharedPersistCollector,
    display_collector: SharedDisplayCollector,
    tool_call_collector: crate::runtime::dispatch::consumer::tool::SharedToolCallCollector,
) -> StepDispatchResult {
    let ctx = input
        .identity
        .as_context(&input.message_id, Some(&input.model), &input.work_id);

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

    let assistant_message = assistant_message_collector.take();
    let tool_calls = tool_call_collector.take();

    for consumer in consumers.iter_mut() {
        consumer
            .on_assistant_message_committed(&ctx, &assistant_message, &tool_calls)
            .await;
    }

    StepDispatchResult {
        step: CompletedStep {
            assistant_message,
            tool_calls,
        },
        local_output: LocalStepOutput {
            display: display_collector.take(),
            persist: persist_collector.take(),
        },
    }
}
