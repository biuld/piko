use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use llmd::gateway::GatewayEvent;
use tokio::sync::mpsc;

use crate::domain::ModelSpec;
use crate::runtime::types::ToolCallItem;
use piko_protocol::{AgentId, Message, MessageId, SessionId, TaskId};

use super::Dispatch;
use super::DispatchSenders;
use super::DisplayEvent;
use super::PersistEvent;
use super::consumer::{
    AgentEventConsumer,
    display::{AssistantMessageState, DisplayChannelConsumer, DisplayCollectingConsumer},
    persist::{AssistantPersistChannelConsumer, AssistantPersistCollectingConsumer},
    tool::ToolExecutionConsumer,
};
use collectors::{SharedAssistantMessageCollector, SharedDisplayCollector, SharedPersistCollector};
use source::{StepDispatchInput, StepDispatchSource, StepFailureInput};

pub mod collectors;
pub mod source;
pub mod stream;

pub struct StepDispatchResult {
    pub assistant_message: Message,
    pub tool_calls: Vec<ToolCallItem>,
    pub display_events: Vec<DisplayEvent>,
    pub persist_events: Vec<PersistEvent>,
}

pub struct StepDispatch {
    name: String,
    source: StepDispatchSource,
    consumers: Vec<Box<dyn AgentEventConsumer>>,
}

impl StepDispatch {
    pub fn from_step_stream(
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        model: ModelSpec,
        events: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
    ) -> Self {
        Self {
            name: format!("agent:{agent_id}"),
            source: StepDispatchSource::StepStream(StepDispatchInput {
                session_id,
                task_id,
                agent_id,
                message_id,
                model,
                events,
            }),
            consumers: Vec::new(),
        }
    }

    pub fn from_step_failure(
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        message_id: MessageId,
        model: ModelSpec,
        error_message: String,
    ) -> Self {
        Self {
            name: format!("agent:{agent_id}"),
            source: StepDispatchSource::StepFailure(StepFailureInput {
                session_id,
                task_id,
                agent_id,
                message_id,
                model,
                error_message,
            }),
            consumers: Vec::new(),
        }
    }

    pub(crate) fn register_consumer<C>(&mut self, consumer: C) -> &mut Self
    where
        C: AgentEventConsumer + 'static,
    {
        self.consumers.push(Box::new(consumer));
        self
    }

    pub(crate) async fn dispatch_step(
        &mut self,
        senders: Option<&DispatchSenders>,
    ) -> StepDispatchResult {
        let (
            assistant_message_collector,
            persist_collector,
            display_collector,
            tool_call_collector,
        ) = self.configure_step_consumers(senders);
        match &mut self.source {
            StepDispatchSource::StepStream(input) => {
                stream::dispatch_step_stream(
                    input,
                    &mut self.consumers,
                    assistant_message_collector,
                    persist_collector,
                    display_collector,
                    tool_call_collector,
                )
                .await
            }
            StepDispatchSource::StepFailure(input) => {
                stream::dispatch_step_failure(
                    input,
                    &mut self.consumers,
                    assistant_message_collector,
                    persist_collector,
                    display_collector,
                    tool_call_collector,
                )
                .await
            }
        }
    }

    fn configure_step_consumers(
        &mut self,
        senders: Option<&DispatchSenders>,
    ) -> (
        SharedAssistantMessageCollector,
        SharedPersistCollector,
        SharedDisplayCollector,
        crate::runtime::dispatch::consumer::tool::SharedToolCallCollector,
    ) {
        let (session_id, task_id, agent_id, message_id) = match &self.source {
            StepDispatchSource::StepStream(i) => (
                i.session_id.clone(),
                i.task_id.clone(),
                i.agent_id.clone(),
                i.message_id.clone(),
            ),
            StepDispatchSource::StepFailure(i) => (
                i.session_id.clone(),
                i.task_id.clone(),
                i.agent_id.clone(),
                i.message_id.clone(),
            ),
        };

        let assistant_message_collector = SharedAssistantMessageCollector::default();
        let persist_collector = SharedPersistCollector::default();
        let display_collector = SharedDisplayCollector::default();
        let tool_call_collector =
            crate::runtime::dispatch::consumer::tool::SharedToolCallCollector::default();

        if let Some(senders) = senders {
            self.register_consumer(DisplayChannelConsumer::new(
                senders.display.clone(),
                AssistantMessageState::new(),
            ));
            self.register_consumer(AssistantPersistChannelConsumer::new(
                senders.persist.clone(),
                assistant_message_collector.clone(),
                AssistantMessageState::new(),
            ));
            self.register_consumer(ToolExecutionConsumer::for_step_dispatch_channel(
                senders.clone(),
                session_id,
                task_id,
                agent_id,
                message_id,
                tool_call_collector.clone(),
            ));
        } else {
            self.register_consumer(DisplayCollectingConsumer::new(
                display_collector.clone(),
                AssistantMessageState::new(),
            ));
            self.register_consumer(AssistantPersistCollectingConsumer::new(
                persist_collector.clone(),
                assistant_message_collector.clone(),
                AssistantMessageState::new(),
            ));
            self.register_consumer(ToolExecutionConsumer::for_step_dispatch_collecting(
                session_id,
                task_id,
                agent_id,
                message_id,
                tool_call_collector.clone(),
                display_collector.clone(),
                persist_collector.clone(),
            ));
        }
        (
            assistant_message_collector,
            persist_collector,
            display_collector,
            tool_call_collector,
        )
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
        let assistant_message_collector;
        let persist_collector;
        let display_collector;
        let tool_call_collector;
        {
            let senders = DispatchSenders {
                persist: persist_tx.clone(),
                display: display_tx.clone(),
                lifecycle: mpsc::unbounded_channel().0,
            };
            (
                assistant_message_collector,
                persist_collector,
                display_collector,
                tool_call_collector,
            ) = self.configure_step_consumers(Some(&senders));
        }
        match &mut self.source {
            StepDispatchSource::StepStream(input) => {
                let _ = stream::dispatch_step_stream(
                    input,
                    &mut self.consumers,
                    assistant_message_collector,
                    persist_collector,
                    display_collector,
                    tool_call_collector,
                )
                .await;
            }
            StepDispatchSource::StepFailure(input) => {
                let _ = stream::dispatch_step_failure(
                    input,
                    &mut self.consumers,
                    assistant_message_collector,
                    persist_collector,
                    display_collector,
                    tool_call_collector,
                )
                .await;
            }
        }
    }
}
