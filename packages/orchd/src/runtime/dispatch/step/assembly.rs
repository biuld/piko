use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::dispatch::consumer::{
    display::{AssistantMessageState, DisplayChannelConsumer, DisplayCollectingConsumer},
    persist::{AssistantPersistChannelConsumer, AssistantPersistCollectingConsumer},
    tool::{SharedToolCallCollector, ToolCallDispatchConsumer},
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
    pub(crate) fn attach(
        dispatch: &mut StepDispatch,
        source: &super::source::StepDispatchMetadata,
        senders: Option<&DispatchSenders>,
    ) -> Self {
        let bundle = Self::default();

        if let Some(senders) = senders {
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
                source.identity.session_id().clone(),
                bundle.tool_call_collector.clone(),
            )));
        } else {
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
                source.identity.session_id().clone(),
                bundle.tool_call_collector.clone(),
                bundle.display_collector.clone(),
                bundle.persist_collector.clone(),
            )));
        }

        bundle
    }
}


