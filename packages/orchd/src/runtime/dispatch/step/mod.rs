use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use llmd::gateway::GatewayEvent;
use tokio::sync::mpsc;

use crate::domain::ModelSpec;
use crate::runtime::dispatch::consumer::DispatchIdentity;
use crate::runtime::types::ToolCallItem;
use piko_protocol::{Message, MessageId};

use super::Dispatch;
use super::DispatchSenders;
use super::DisplayEvent;
use super::PersistEvent;
use super::consumer::StepEventConsumer;
use source::{StepDispatchInput, StepDispatchSource, StepFailureInput};

mod assembly;
pub mod collectors;
pub mod source;
pub mod stream;

pub struct CompletedStep {
    pub assistant_message: Message,
    pub tool_calls: Vec<ToolCallItem>,
}

pub struct LocalStepOutput {
    pub display: Vec<DisplayEvent>,
    pub persist: Vec<PersistEvent>,
}

pub struct StepDispatchResult {
    pub step: CompletedStep,
    pub local_output: LocalStepOutput,
}

pub struct StepDispatch {
    name: String,
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
            name: format!("agent:{}", identity.agent_id()),
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
            name: format!("agent:{}", identity.agent_id()),
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
        senders: Option<&DispatchSenders>,
    ) -> StepDispatchResult {
        let metadata = self.source.metadata();
        let bundle = assembly::StepConsumerBundle::attach(self, &metadata, senders);
        match &mut self.source {
            StepDispatchSource::StepStream(input) => {
                stream::dispatch_step_stream(
                    input,
                    &mut self.consumers,
                    bundle.assistant_message_collector,
                    bundle.persist_collector,
                    bundle.display_collector,
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
                    bundle.display_collector,
                    bundle.tool_call_collector,
                )
                .await
            }
        }
    }
}

#[async_trait]
impl Dispatch for StepDispatch {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(
        &mut self,
        persist_tx: mpsc::Sender<Arc<PersistEvent>>,
        display_tx: mpsc::Sender<Arc<DisplayEvent>>,
        lifecycle_tx: Option<mpsc::Sender<Arc<super::LifecycleEvent>>>,
    ) {
        let _ = lifecycle_tx;
        let senders = DispatchSenders {
            persist: persist_tx.clone(),
            display: display_tx.clone(),
            lifecycle: mpsc::unbounded_channel().0,
        };
        let _ = self.dispatch_step(Some(&senders)).await;
    }
}
