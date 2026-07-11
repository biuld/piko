use crate::runtime::events::delta_lane::{AssistantMessageState, RealtimeCollectingConsumer};
use crate::runtime::events::event_lane::AssistantPersistCollectingConsumer;
use crate::runtime::events::{
    TaskEventEmitter,
    step_consumers::{EmitterPersistConsumer, EmitterRealtimeConsumer},
};
use crate::runtime::tools::{SharedToolCallCollector, ToolCallDispatchConsumer};

use super::StepDispatch;
use crate::runtime::events::collector::{
    SharedAssistantMessageCollector, SharedPersistCollector, SharedRealtimeCollector,
};

#[derive(Default)]
pub(crate) struct StepConsumerBundle {
    pub(crate) assistant_message_collector: SharedAssistantMessageCollector,
    pub(crate) persist_collector: SharedPersistCollector,
    pub(crate) realtime_collector: SharedRealtimeCollector,
    pub(crate) tool_call_collector: SharedToolCallCollector,
}

impl StepConsumerBundle {
    pub(crate) fn attach_emitter(
        dispatch: &mut StepDispatch,
        source: &super::source::StepDispatchMetadata,
        emitter: &TaskEventEmitter,
    ) -> Self {
        let bundle = Self::default();
        let emitter = emitter.clone();

        dispatch.push_boxed_consumer(Box::new(EmitterRealtimeConsumer::new(
            emitter.clone(),
            bundle.realtime_collector.clone(),
        )));
        dispatch.push_boxed_consumer(Box::new(EmitterPersistConsumer::new(
            emitter.clone(),
            bundle.persist_collector.clone(),
            bundle.assistant_message_collector.clone(),
        )));
        dispatch.push_boxed_consumer(Box::new(ToolCallDispatchConsumer::for_emitter(
            emitter,
            source.identity.clone(),
            bundle.tool_call_collector.clone(),
        )));

        bundle
    }

    pub(crate) fn attach_collecting(
        dispatch: &mut StepDispatch,
        source: &super::source::StepDispatchMetadata,
    ) -> Self {
        let bundle = Self::default();

        dispatch.push_boxed_consumer(Box::new(RealtimeCollectingConsumer::new(
            bundle.realtime_collector.clone(),
            AssistantMessageState::new(),
        )));
        dispatch.push_boxed_consumer(Box::new(AssistantPersistCollectingConsumer::new(
            bundle.persist_collector.clone(),
            bundle.assistant_message_collector.clone(),
            AssistantMessageState::new(),
        )));
        dispatch.push_boxed_consumer(Box::new(ToolCallDispatchConsumer::for_collecting(
            source.identity.clone(),
            bundle.tool_call_collector.clone(),
            bundle.realtime_collector.clone(),
            bundle.persist_collector.clone(),
        )));

        bundle
    }
}
