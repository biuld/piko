use std::pin::Pin;

use futures_core::Stream;
use llmd::gateway::GatewayEvent;

use crate::domain::model::step::ModelSpec;
use crate::domain::tools::call::ToolCallItem;
use crate::runtime::events::identity::{DispatchIdentity, StepEventConsumer};
use piko_protocol::{Message, MessageId, PersistEvent};

use crate::domain::RealtimeFrame;

use crate::runtime::events::TaskEventEmitter;
use source::{StepDispatchInput, StepDispatchSource, StepFailureInput};

mod assembly;
pub mod source;
pub mod stream;

#[cfg(test)]
mod tests;

pub struct CompletedStep {
    pub assistant_message: Message,
    pub tool_calls: Vec<ToolCallItem>,
}

pub struct LocalStepOutput {
    pub realtime: Vec<RealtimeFrame>,
    pub persist: Vec<PersistEvent>,
}

pub struct StepDispatchResult {
    pub step: CompletedStep,
    pub local_output: LocalStepOutput,
}

pub struct StepDispatch {
    source: StepDispatchSource,
    consumers: Vec<Box<dyn StepEventConsumer>>,
}

impl StepDispatch {
    pub(crate) fn from_step_stream(
        identity: DispatchIdentity,
        message_id: MessageId,
        work_id: String,
        model: ModelSpec,
        events: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
    ) -> Self {
        Self {
            source: StepDispatchSource::StepStream(StepDispatchInput {
                identity,
                message_id,
                work_id,
                model,
                events,
            }),
            consumers: Vec::new(),
        }
    }

    pub(crate) fn from_step_failure(
        identity: DispatchIdentity,
        message_id: MessageId,
        work_id: String,
        model: ModelSpec,
        error_message: String,
    ) -> Self {
        Self {
            source: StepDispatchSource::StepFailure(StepFailureInput {
                identity,
                message_id,
                work_id,
                model,
                error_message,
            }),
            consumers: Vec::new(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn register_consumer<C>(&mut self, consumer: C) -> &mut Self
    where
        C: StepEventConsumer + 'static,
    {
        self.push_boxed_consumer(Box::new(consumer));
        self
    }

    pub(super) fn push_boxed_consumer(&mut self, consumer: Box<dyn StepEventConsumer>) {
        self.consumers.push(consumer);
    }

    pub(crate) async fn dispatch_step(
        &mut self,
        emitter: Option<&TaskEventEmitter>,
    ) -> StepDispatchResult {
        let metadata = self.source.metadata();
        let bundle = match emitter {
            Some(emitter) => assembly::StepConsumerBundle::attach_emitter(self, &metadata, emitter),
            None => assembly::StepConsumerBundle::attach_collecting(self, &metadata),
        };
        match &mut self.source {
            StepDispatchSource::StepStream(input) => {
                stream::dispatch_step_stream(
                    input,
                    &mut self.consumers,
                    bundle.assistant_message_collector,
                    bundle.persist_collector,
                    bundle.realtime_collector,
                    bundle.tool_call_collector,
                )
                .await
            }
            StepDispatchSource::StepFailure(input) => {
                stream::dispatch_step_failure(
                    input,
                    &mut self.consumers,
                    bundle.assistant_message_collector,
                    bundle.persist_collector,
                    bundle.realtime_collector,
                    bundle.tool_call_collector,
                )
                .await
            }
        }
    }
}
