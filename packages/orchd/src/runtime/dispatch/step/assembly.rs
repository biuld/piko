use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::dispatch::consumer::{
    display::{AssistantMessageState, DisplayChannelConsumer, DisplayCollectingConsumer},
    persist::{AssistantPersistChannelConsumer, AssistantPersistCollectingConsumer},
    tool::{SharedToolCallCollector, ToolCallDispatchConsumer},
};
use crate::runtime::events::{
    TaskEventEmitter,
    step_consumers::{EmitterDisplayConsumer, EmitterPersistConsumer},
};

use super::StepDispatch;
use super::collectors::{
    SharedAssistantMessageCollector, SharedDisplayCollector, SharedPersistCollector,
};

#[derive(Default)]
pub(crate) struct StepConsumerBundle {
    pub(crate) assistant_message_collector: SharedAssistantMessageCollector,
    pub(crate) persist_collector: SharedPersistCollector,
    pub(crate) display_collector: SharedDisplayCollector,
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

        dispatch.push_boxed_consumer(Box::new(EmitterDisplayConsumer::new(
            emitter.clone(),
            bundle.display_collector.clone(),
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

        dispatch.push_boxed_consumer(Box::new(DisplayCollectingConsumer::new(
            bundle.display_collector.clone(),
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
            bundle.display_collector.clone(),
            bundle.persist_collector.clone(),
        )));

        bundle
    }

    #[allow(dead_code)]
    pub(crate) fn attach_legacy_channels(
        dispatch: &mut StepDispatch,
        source: &super::source::StepDispatchMetadata,
        senders: &DispatchSenders,
    ) -> Self {
        let bundle = Self::default();

        dispatch.push_boxed_consumer(Box::new(DisplayChannelConsumer::new(
            senders.display.clone(),
            AssistantMessageState::new(),
        )));
        dispatch.push_boxed_consumer(Box::new(AssistantPersistChannelConsumer::new(
            senders.persist.clone(),
            bundle.assistant_message_collector.clone(),
            AssistantMessageState::new(),
        )));
        dispatch.push_boxed_consumer(Box::new(ToolCallDispatchConsumer::for_channel(
            senders.clone(),
            source.identity.clone(),
            bundle.tool_call_collector.clone(),
        )));

        bundle
    }
}
