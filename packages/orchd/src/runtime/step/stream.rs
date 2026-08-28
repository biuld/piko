use futures_util::StreamExt;
use piko_llmd::gateway::{ErrorClass, InferenceError, InferenceEvent};

use super::source::{StepDispatchInput, StepFailureInput};
use super::{CompletedStep, LocalStepOutput, StepDispatchResult, StepTermination};
use crate::runtime::events::collector::{
    SharedAssistantMessageCollector, SharedPersistCollector, SharedRealtimeCollector,
};
use crate::runtime::events::identity::StepEventConsumer;
use crate::runtime::tools::SharedToolCallCollector;

pub(crate) async fn dispatch_step_stream(
    input: &mut StepDispatchInput,
    consumers: &mut Vec<Box<dyn StepEventConsumer>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    persist_collector: SharedPersistCollector,
    realtime_collector: SharedRealtimeCollector,
    tool_call_collector: SharedToolCallCollector,
) -> StepDispatchResult {
    let ctx =
        input
            .identity
            .as_context(&input.message_id, Some(&input.model), &input.source_turn_id);

    for consumer in consumers.iter_mut() {
        consumer.on_step_started(&ctx).await;
    }

    let mut termination = None;
    while let Some(event) = input.events.next().await {
        for consumer in consumers.iter_mut() {
            consumer.on_gateway_event(&ctx, &event).await;
        }

        match event {
            InferenceEvent::Completed(piko_llmd::gateway::FinishReason::Failed { message }) => {
                termination = Some(StepTermination::Failed(message));
                break;
            }
            InferenceEvent::Completed(piko_llmd::gateway::FinishReason::Cancelled) => {
                termination = Some(StepTermination::Cancelled);
                break;
            }
            InferenceEvent::Completed(_) => {
                termination = Some(StepTermination::Completed);
                break;
            }
            InferenceEvent::Error(error) => {
                termination = Some(StepTermination::Failed(error.to_string()));
                break;
            }
            _ => {}
        }
    }

    if termination.is_none() {
        let error = InferenceError::new(
            ErrorClass::Upstream,
            &input.model.provider,
            "stream",
            "model stream ended without a terminal event",
        );
        for consumer in consumers.iter_mut() {
            consumer
                .on_gateway_event(&ctx, &InferenceEvent::Error(error.clone()))
                .await;
        }
        termination = Some(StepTermination::Failed(error.to_string()));
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
        termination: termination.expect("step stream termination assigned above"),
        local_output: LocalStepOutput {
            realtime: realtime_collector.take(),
            persist: persist_collector.take(),
        },
    }
}

pub(crate) async fn dispatch_step_failure(
    input: &mut StepFailureInput,
    consumers: &mut Vec<Box<dyn StepEventConsumer>>,
    assistant_message_collector: SharedAssistantMessageCollector,
    persist_collector: SharedPersistCollector,
    realtime_collector: SharedRealtimeCollector,
    tool_call_collector: SharedToolCallCollector,
) -> StepDispatchResult {
    let ctx =
        input
            .identity
            .as_context(&input.message_id, Some(&input.model), &input.source_turn_id);

    for consumer in consumers.iter_mut() {
        consumer.on_step_started(&ctx).await;
    }

    let error_event = InferenceEvent::Error(InferenceError::new(
        ErrorClass::Upstream,
        &input.model.provider,
        "execute",
        input.error_message.clone(),
    ));
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
        termination: StepTermination::Failed(input.error_message.clone()),
        local_output: LocalStepOutput {
            realtime: realtime_collector.take(),
            persist: persist_collector.take(),
        },
    }
}
