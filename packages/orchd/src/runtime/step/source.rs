use std::pin::Pin;

use futures_core::Stream;
use llmd::gateway::GatewayEvent;

use crate::domain::model::step::ModelSpec;
use crate::runtime::events::identity::DispatchIdentity;
use piko_protocol::MessageId;

pub(crate) struct StepDispatchMetadata {
    pub(crate) identity: DispatchIdentity,
}

pub(crate) enum StepDispatchSource {
    StepStream(StepDispatchInput),
    StepFailure(StepFailureInput),
}

impl StepDispatchSource {
    pub(crate) fn metadata(&self) -> StepDispatchMetadata {
        match self {
            Self::StepStream(input) => StepDispatchMetadata {
                identity: input.identity.clone(),
            },
            Self::StepFailure(input) => StepDispatchMetadata {
                identity: input.identity.clone(),
            },
        }
    }
}

pub(crate) struct StepDispatchInput {
    pub(crate) identity: DispatchIdentity,
    pub(crate) message_id: MessageId,
    pub(crate) source_turn_id: String,
    pub(crate) model: ModelSpec,
    pub(crate) events: Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>,
}

pub(crate) struct StepFailureInput {
    pub(crate) identity: DispatchIdentity,
    pub(crate) message_id: MessageId,
    pub(crate) source_turn_id: String,
    pub(crate) model: ModelSpec,
    pub(crate) error_message: String,
}
