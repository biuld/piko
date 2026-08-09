use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gateway::{ErrorClass, GatewayError, ModelOutputMetadata};
use crate::modeling::ProtocolKind;
use crate::target::ModelTarget;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct EncryptedReasoningItem {
    pub item_id: String,
    pub encrypted_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ResponsesContinuation {
    pub response_id: String,
    pub output_item_ids: Vec<String>,
    pub call_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub encrypted_reasoning: Vec<EncryptedReasoningItem>,
}

pub(super) fn decode_continuation(
    envelope: &piko_protocol::ModelContinuation,
    target: &ModelTarget,
) -> Result<Option<ResponsesContinuation>, GatewayError> {
    if envelope.adapter != ProtocolKind::Responses.adapter_id() {
        return Ok(None);
    }
    serde_json::from_value(envelope.state.clone())
        .map(Some)
        .map_err(|error| protocol(target, format!("invalid Responses continuation: {error}")))
}

pub(super) fn string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

pub(super) fn protocol(target: &ModelTarget, message: impl Into<String>) -> GatewayError {
    GatewayError::new(
        ErrorClass::Protocol,
        &target.id,
        "decode_responses",
        message,
    )
}

pub(super) fn output_metadata(
    response_id: String,
    output_item_ids: Vec<String>,
    call_ids: Vec<String>,
    encrypted_reasoning: Vec<EncryptedReasoningItem>,
) -> ModelOutputMetadata {
    let state = serde_json::to_value(ResponsesContinuation {
        response_id,
        output_item_ids,
        call_ids,
        encrypted_reasoning,
    })
    .expect("Responses continuation contains only serializable values");
    ModelOutputMetadata {
        continuation: Some(piko_protocol::ModelContinuation {
            adapter: ProtocolKind::Responses.adapter_id().into(),
            state,
        }),
    }
}
